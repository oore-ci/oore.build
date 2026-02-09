use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use aws_config::BehaviorVersion;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::presigning::PresigningConfig;
use oore_contract::{
    ArtifactStorageProvider, ArtifactStorageSettings, ArtifactStorageSource,
};
use sqlx::Row;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::crypto;
use crate::token::{generate_token, hash_token};
use crate::util::now_unix;

#[derive(Debug, Clone)]
pub struct StorageConfig {
    pub provider: ArtifactStorageProvider,
    pub endpoint: Option<String>,
    pub bucket: String,
    pub region: String,
    pub access_key_id: String,
    pub secret_access_key: String,
}

impl StorageConfig {
    /// Read storage configuration from environment variables.
    ///
    /// Required: `OORE_S3_BUCKET`, `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`.
    /// Optional: `OORE_S3_ENDPOINT` (for MinIO/custom), `OORE_S3_REGION` (default: "us-east-1").
    pub fn from_env() -> Result<Self, anyhow::Error> {
        let bucket = std::env::var("OORE_S3_BUCKET")
            .map_err(|_| anyhow::anyhow!("OORE_S3_BUCKET not set"))?;
        let access_key_id = std::env::var("AWS_ACCESS_KEY_ID")
            .map_err(|_| anyhow::anyhow!("AWS_ACCESS_KEY_ID not set"))?;
        let secret_access_key = std::env::var("AWS_SECRET_ACCESS_KEY")
            .map_err(|_| anyhow::anyhow!("AWS_SECRET_ACCESS_KEY not set"))?;

        let endpoint = std::env::var("OORE_S3_ENDPOINT").ok();
        let region = std::env::var("OORE_S3_REGION").unwrap_or_else(|_| "us-east-1".to_string());

        Ok(Self {
            provider: ArtifactStorageProvider::S3,
            endpoint,
            bucket,
            region,
            access_key_id,
            secret_access_key,
        })
    }
}

pub struct StorageClient {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl StorageClient {
    pub fn new(config: StorageConfig) -> Self {
        let credentials =
            Credentials::new(&config.access_key_id, &config.secret_access_key, None, None, "oore");

        let mut s3_config_builder = aws_sdk_s3::config::Builder::new()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new(config.region))
            .credentials_provider(credentials)
            .force_path_style(true);

        if let Some(ref endpoint) = config.endpoint {
            s3_config_builder = s3_config_builder.endpoint_url(endpoint);
        }

        let s3_config = s3_config_builder.build();
        let client = aws_sdk_s3::Client::from_conf(s3_config);

        info!(bucket = %config.bucket, endpoint = ?config.endpoint, provider = %config.provider, "S3-compatible storage client initialized");

        Self {
            client,
            bucket: config.bucket,
        }
    }

    pub async fn generate_upload_url(
        &self,
        key: &str,
        ttl_secs: u64,
    ) -> Result<String, anyhow::Error> {
        let presigning_config = PresigningConfig::builder()
            .expires_in(Duration::from_secs(ttl_secs))
            .build()?;

        let presigned = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(presigning_config)
            .await?;

        Ok(presigned.uri().to_string())
    }

    pub async fn generate_download_url(
        &self,
        key: &str,
        ttl_secs: u64,
    ) -> Result<String, anyhow::Error> {
        let presigning_config = PresigningConfig::builder()
            .expires_in(Duration::from_secs(ttl_secs))
            .build()?;

        let presigned = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(presigning_config)
            .await?;

        Ok(presigned.uri().to_string())
    }
}

#[derive(Debug, Clone)]
struct LocalTokenEntry {
    key: String,
    expires_at: i64,
}

pub struct LocalStorageClient {
    base_dir: PathBuf,
    public_base_url: String,
    upload_tokens: Mutex<HashMap<String, LocalTokenEntry>>,
    download_tokens: Mutex<HashMap<String, LocalTokenEntry>>,
}

impl LocalStorageClient {
    pub fn new(base_dir: PathBuf) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&base_dir).with_context(|| {
            format!(
                "failed to create local artifacts directory: {}",
                base_dir.display()
            )
        })?;

        let public_base_url = std::env::var("OORE_PUBLIC_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8787".to_string())
            .trim_end_matches('/')
            .to_string();

        info!(base_dir = %base_dir.display(), public_base_url = %public_base_url, "local artifact storage initialized");

        Ok(Self {
            base_dir,
            public_base_url,
            upload_tokens: Mutex::new(HashMap::new()),
            download_tokens: Mutex::new(HashMap::new()),
        })
    }

    fn sanitize_rel_path(key: &str) -> anyhow::Result<PathBuf> {
        let mut rel = PathBuf::new();
        for component in Path::new(key).components() {
            match component {
                Component::Normal(part) => rel.push(part),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(anyhow::anyhow!("invalid artifact key path"));
                }
            }
        }

        if rel.as_os_str().is_empty() {
            return Err(anyhow::anyhow!("invalid artifact key path"));
        }

        Ok(rel)
    }

    fn full_path_for_key(&self, key: &str) -> anyhow::Result<PathBuf> {
        let rel = Self::sanitize_rel_path(key)?;
        Ok(self.base_dir.join(rel))
    }

    async fn issue_token(
        map: &Mutex<HashMap<String, LocalTokenEntry>>,
        key: &str,
        ttl_secs: u64,
    ) -> String {
        let token = generate_token();
        let token_hash = hash_token(&token);
        let now = now_unix();
        let mut guard = map.lock().await;
        guard.retain(|_, entry| entry.expires_at > now);
        guard.insert(
            token_hash,
            LocalTokenEntry {
                key: key.to_string(),
                expires_at: now + ttl_secs as i64,
            },
        );
        token
    }

    pub async fn generate_upload_url(&self, key: &str, ttl_secs: u64) -> String {
        let token = Self::issue_token(&self.upload_tokens, key, ttl_secs).await;
        format!("{}/v1/artifacts/local-upload/{}", self.public_base_url, token)
    }

    pub async fn generate_download_url(&self, key: &str, ttl_secs: u64) -> String {
        let token = Self::issue_token(&self.download_tokens, key, ttl_secs).await;
        format!("{}/v1/artifacts/download/{}", self.public_base_url, token)
    }

    pub async fn handle_upload(&self, token: &str, bytes: &[u8]) -> anyhow::Result<bool> {
        let token_hash = hash_token(token);
        let now = now_unix();
        let entry = {
            let mut guard = self.upload_tokens.lock().await;
            guard.retain(|_, v| v.expires_at > now);
            guard.remove(&token_hash)
        };

        let Some(entry) = entry else {
            return Ok(false);
        };

        let path = self.full_path_for_key(&entry.key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.with_context(|| {
                format!("failed to create parent directory: {}", parent.display())
            })?;
        }

        tokio::fs::write(&path, bytes)
            .await
            .with_context(|| format!("failed to write artifact file: {}", path.display()))?;

        Ok(true)
    }

    pub async fn handle_download(
        &self,
        token: &str,
    ) -> anyhow::Result<Option<LocalDownloadPayload>> {
        let token_hash = hash_token(token);
        let now = now_unix();

        let entry = {
            let mut guard = self.download_tokens.lock().await;
            guard.retain(|_, v| v.expires_at > now);
            guard.get(&token_hash).cloned()
        };

        let Some(entry) = entry else {
            return Ok(None);
        };

        let path = self.full_path_for_key(&entry.key)?;
        let bytes = match tokio::fs::read(&path).await {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(e).with_context(|| format!("failed to read {}", path.display()));
            }
        };

        let file_name = Path::new(&entry.key)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("artifact.bin")
            .to_string();

        Ok(Some(LocalDownloadPayload { bytes, file_name }))
    }

    pub fn base_dir(&self) -> String {
        self.base_dir.to_string_lossy().to_string()
    }
}

pub struct LocalDownloadPayload {
    pub bytes: Vec<u8>,
    pub file_name: String,
}

pub enum StorageBackend {
    Disabled,
    S3(StorageClient),
    Local(Arc<LocalStorageClient>),
}

impl StorageBackend {
    pub async fn generate_upload_url(
        &self,
        key: &str,
        ttl_secs: u64,
    ) -> Result<Option<String>, anyhow::Error> {
        match self {
            Self::Disabled => Ok(None),
            Self::S3(client) => client.generate_upload_url(key, ttl_secs).await.map(Some),
            Self::Local(client) => Ok(Some(client.generate_upload_url(key, ttl_secs).await)),
        }
    }

    pub async fn generate_download_url(
        &self,
        key: &str,
        ttl_secs: u64,
    ) -> Result<Option<String>, anyhow::Error> {
        match self {
            Self::Disabled => Ok(None),
            Self::S3(client) => client.generate_download_url(key, ttl_secs).await.map(Some),
            Self::Local(client) => Ok(Some(client.generate_download_url(key, ttl_secs).await)),
        }
    }

    pub async fn handle_local_upload(
        &self,
        token: &str,
        bytes: &[u8],
    ) -> anyhow::Result<bool> {
        match self {
            Self::Local(client) => client.handle_upload(token, bytes).await,
            _ => Ok(false),
        }
    }

    pub async fn handle_local_download(
        &self,
        token: &str,
    ) -> anyhow::Result<Option<LocalDownloadPayload>> {
        match self {
            Self::Local(client) => client.handle_download(token).await,
            _ => Ok(None),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EffectiveStorageConfig {
    pub provider: ArtifactStorageProvider,
    pub source: ArtifactStorageSource,
    pub local_base_dir: Option<String>,
    pub s3_bucket: Option<String>,
    pub s3_region: Option<String>,
    pub s3_endpoint: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub updated_at: Option<i64>,
}

impl EffectiveStorageConfig {
    pub fn to_public_settings(&self) -> ArtifactStorageSettings {
        ArtifactStorageSettings {
            provider: self.provider,
            local_base_dir: self.local_base_dir.clone(),
            s3_bucket: self.s3_bucket.clone(),
            s3_region: self.s3_region.clone(),
            s3_endpoint: self.s3_endpoint.clone(),
            has_access_key_id: self.access_key_id.as_ref().is_some_and(|v| !v.is_empty()),
            has_secret_access_key: self
                .secret_access_key
                .as_ref()
                .is_some_and(|v| !v.is_empty()),
            source: self.source,
            updated_at: self.updated_at,
        }
    }
}

fn default_local_artifacts_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("oore").join("artifacts")
}

pub fn build_backend_from_config(config: &EffectiveStorageConfig) -> anyhow::Result<StorageBackend> {
    match config.provider {
        ArtifactStorageProvider::Disabled => Ok(StorageBackend::Disabled),
        ArtifactStorageProvider::Local => {
            let base_dir = config
                .local_base_dir
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(default_local_artifacts_dir);
            let client = LocalStorageClient::new(base_dir)?;
            Ok(StorageBackend::Local(Arc::new(client)))
        }
        ArtifactStorageProvider::S3 | ArtifactStorageProvider::R2 => {
            let bucket = config
                .s3_bucket
                .as_ref()
                .filter(|v| !v.trim().is_empty())
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("s3_bucket is required"))?;
            let access_key_id = config
                .access_key_id
                .as_ref()
                .filter(|v| !v.trim().is_empty())
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("access_key_id is required"))?;
            let secret_access_key = config
                .secret_access_key
                .as_ref()
                .filter(|v| !v.trim().is_empty())
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("secret_access_key is required"))?;
            let region = config
                .s3_region
                .clone()
                .unwrap_or_else(|| "us-east-1".to_string());

            let storage_config = StorageConfig {
                provider: config.provider,
                endpoint: config.s3_endpoint.clone(),
                bucket,
                region,
                access_key_id,
                secret_access_key,
            };

            Ok(StorageBackend::S3(StorageClient::new(storage_config)))
        }
    }
}

pub async fn load_effective_config(
    pool: &sqlx::SqlitePool,
    encryption_key: &[u8],
) -> anyhow::Result<EffectiveStorageConfig> {
    let row = sqlx::query(
        "SELECT provider, local_base_dir, s3_bucket, s3_region, s3_endpoint, \
         s3_access_key_encrypted, s3_secret_key_encrypted, updated_at \
         FROM artifact_storage_settings WHERE id = 1",
    )
    .fetch_optional(pool)
    .await
    .context("failed to load artifact storage settings")?;

    if let Some(row) = row {
        let provider_str: String = row.get("provider");
        let provider = ArtifactStorageProvider::from_str(&provider_str)
            .map_err(|e| anyhow::anyhow!(e))?;

        let access_key_id = match row.get::<Option<String>, _>("s3_access_key_encrypted") {
            Some(encrypted) => Some(
                crypto::decrypt(&encrypted, encryption_key)
                    .context("failed to decrypt s3 access key")?,
            ),
            None => None,
        };

        let secret_access_key = match row.get::<Option<String>, _>("s3_secret_key_encrypted") {
            Some(encrypted) => Some(
                crypto::decrypt(&encrypted, encryption_key)
                    .context("failed to decrypt s3 secret key")?,
            ),
            None => None,
        };

        return Ok(EffectiveStorageConfig {
            provider,
            source: ArtifactStorageSource::Database,
            local_base_dir: row.get("local_base_dir"),
            s3_bucket: row.get("s3_bucket"),
            s3_region: row.get("s3_region"),
            s3_endpoint: row.get("s3_endpoint"),
            access_key_id,
            secret_access_key,
            updated_at: row.get("updated_at"),
        });
    }

    if let Ok(env_cfg) = StorageConfig::from_env() {
        return Ok(EffectiveStorageConfig {
            provider: env_cfg.provider,
            source: ArtifactStorageSource::Environment,
            local_base_dir: None,
            s3_bucket: Some(env_cfg.bucket),
            s3_region: Some(env_cfg.region),
            s3_endpoint: env_cfg.endpoint,
            access_key_id: Some(env_cfg.access_key_id),
            secret_access_key: Some(env_cfg.secret_access_key),
            updated_at: None,
        });
    }

    Ok(EffectiveStorageConfig {
        provider: ArtifactStorageProvider::Disabled,
        source: ArtifactStorageSource::Default,
        local_base_dir: Some(default_local_artifacts_dir().to_string_lossy().to_string()),
        s3_bucket: None,
        s3_region: None,
        s3_endpoint: None,
        access_key_id: None,
        secret_access_key: None,
        updated_at: None,
    })
}

pub async fn load_backend(
    pool: &sqlx::SqlitePool,
    encryption_key: &[u8],
) -> StorageBackend {
    match load_effective_config(pool, encryption_key).await {
        Ok(cfg) => match build_backend_from_config(&cfg) {
            Ok(backend) => backend,
            Err(e) => {
                error!(error = %e, "failed to build artifact storage backend; using disabled backend");
                StorageBackend::Disabled
            }
        },
        Err(e) => {
            warn!(error = %e, "failed to load artifact storage config; using disabled backend");
            StorageBackend::Disabled
        }
    }
}
