use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
use aws_sdk_s3::presigning::PresigningConfig;
use axum::body::{Body, Bytes};
use oore_contract::{ArtifactStorageProvider, ArtifactStorageSettings, ArtifactStorageSource};
use sqlx::Row;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Mutex, Semaphore};
use tokio_stream::StreamExt;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::crypto;
use crate::token::{generate_token, hash_token};
use crate::util::{now_unix, resolve_oored_data_dir};

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

#[derive(Clone)]
pub struct StorageClient {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl StorageClient {
    pub fn new(config: StorageConfig) -> anyhow::Result<Self> {
        if let Some(endpoint) = config.endpoint.as_deref() {
            validate_s3_transport_url(endpoint)?;
        }

        let credentials = Credentials::new(
            &config.access_key_id,
            &config.secret_access_key,
            None,
            None,
            "oore",
        );

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

        Ok(Self {
            client,
            bucket: config.bucket,
        })
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

        let url = presigned.uri().to_string();
        validate_s3_transport_url(&url)?;
        Ok(url)
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

        let url = presigned.uri().to_string();
        validate_s3_transport_url(&url)?;
        Ok(url)
    }

    pub async fn delete_object(&self, key: &str) -> Result<(), anyhow::Error> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .with_context(|| format!("failed to delete S3 object: {key}"))?;
        Ok(())
    }
}

fn validate_s3_transport_url(value: &str) -> anyhow::Result<()> {
    let parsed = url::Url::parse(value).context("invalid S3 endpoint URL")?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        anyhow::bail!("S3 endpoint URL must not include credentials");
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("S3 endpoint URL must include a host"))?;
    let loopback_host = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);

    match parsed.scheme() {
        "https" => Ok(()),
        "http" if crate::is_loopback_host(loopback_host) => Ok(()),
        "http" => anyhow::bail!("non-loopback S3 endpoints must use HTTPS"),
        _ => anyhow::bail!("S3 endpoint URL must use HTTP or HTTPS"),
    }
}

#[derive(Debug, Clone)]
struct LocalTokenEntry {
    key: String,
    expires_at: i64,
    sequence: u64,
}

const MAX_LOCAL_TOKENS: usize = 4096;
const MAX_CONCURRENT_LOCAL_DOWNLOADS: usize = 16;
const LOCAL_DOWNLOAD_CHUNK_BYTES: usize = 64 * 1024;

struct LocalTokenStore {
    entries: HashMap<String, LocalTokenEntry>,
    expiry_order: BTreeMap<(i64, u64), String>,
    capacity: usize,
    next_sequence: u64,
}

impl LocalTokenStore {
    fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::with_capacity(capacity),
            expiry_order: BTreeMap::new(),
            capacity,
            next_sequence: 0,
        }
    }

    fn purge_expired(&mut self, now: i64) {
        while let Some((expires_at, token_hash)) = self
            .expiry_order
            .first_key_value()
            .map(|((expires_at, _), token_hash)| (*expires_at, token_hash.clone()))
        {
            if expires_at > now {
                break;
            }
            self.expiry_order.pop_first();
            self.entries.remove(&token_hash);
        }
    }

    fn insert(&mut self, token_hash: String, mut entry: LocalTokenEntry, now: i64) {
        self.purge_expired(now);
        while self.entries.len() >= self.capacity {
            let Some(((expires_at, _), oldest_hash)) = self.expiry_order.pop_first() else {
                break;
            };
            self.entries.remove(&oldest_hash);
            debug_assert!(expires_at > now);
        }
        entry.sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        self.expiry_order
            .insert((entry.expires_at, entry.sequence), token_hash.clone());
        self.entries.insert(token_hash, entry);
    }

    fn get(&mut self, token_hash: &str, now: i64) -> Option<LocalTokenEntry> {
        self.purge_expired(now);
        self.entries.get(token_hash).cloned()
    }

    fn remove(&mut self, token_hash: &str, now: i64) -> Option<LocalTokenEntry> {
        self.purge_expired(now);
        let entry = self.entries.remove(token_hash)?;
        self.expiry_order
            .remove(&(entry.expires_at, entry.sequence));
        Some(entry)
    }
}

pub struct LocalStorageClient {
    base_dir: PathBuf,
    public_base_url: String,
    upload_tokens: Mutex<LocalTokenStore>,
    download_tokens: Mutex<LocalTokenStore>,
    download_slots: Arc<Semaphore>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalUploadOutcome {
    Stored,
    InvalidToken,
    TooLarge,
}

impl LocalStorageClient {
    pub fn new(base_dir: PathBuf, public_base_url: Option<String>) -> anyhow::Result<Self> {
        Self::new_with_limits(
            base_dir,
            public_base_url,
            MAX_LOCAL_TOKENS,
            MAX_CONCURRENT_LOCAL_DOWNLOADS,
        )
    }

    fn new_with_limits(
        base_dir: PathBuf,
        public_base_url: Option<String>,
        token_capacity: usize,
        download_concurrency: usize,
    ) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&base_dir).with_context(|| {
            format!(
                "failed to create local artifacts directory: {}",
                base_dir.display()
            )
        })?;

        let public_base_url = public_base_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("http://127.0.0.1:8787")
            .trim_end_matches('/')
            .to_string();

        info!(base_dir = %base_dir.display(), public_base_url = %public_base_url, "local artifact storage initialized");

        Ok(Self {
            base_dir,
            public_base_url,
            upload_tokens: Mutex::new(LocalTokenStore::new(token_capacity)),
            download_tokens: Mutex::new(LocalTokenStore::new(token_capacity)),
            download_slots: Arc::new(Semaphore::new(download_concurrency)),
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

    async fn issue_token(map: &Mutex<LocalTokenStore>, key: &str, ttl_secs: u64) -> String {
        let token = generate_token();
        let token_hash = hash_token(&token);
        let now = now_unix();
        let mut guard = map.lock().await;
        guard.insert(
            token_hash,
            LocalTokenEntry {
                key: key.to_string(),
                expires_at: now + ttl_secs as i64,
                sequence: 0,
            },
            now,
        );
        token
    }

    pub async fn generate_upload_url(&self, key: &str, ttl_secs: u64) -> String {
        let token = Self::issue_token(&self.upload_tokens, key, ttl_secs).await;
        format!(
            "{}/v1/artifacts/local-upload/{}",
            self.public_base_url, token
        )
    }

    pub async fn generate_download_url(&self, key: &str, ttl_secs: u64) -> String {
        let token = Self::issue_token(&self.download_tokens, key, ttl_secs).await;
        // Resolve browser downloads against the requesting instance, including custom ports.
        format!("/v1/artifacts/download/{token}")
    }

    pub async fn generate_download_url_with_base(
        &self,
        key: &str,
        ttl_secs: u64,
        public_base_url: Option<&str>,
        query_pair: Option<(&str, &str)>,
    ) -> String {
        let token = Self::issue_token(&self.download_tokens, key, ttl_secs).await;
        let base = public_base_url
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("")
            .trim_end_matches('/');
        let mut url = format!("{base}/install/download/{token}");
        if let Some((key, value)) = query_pair {
            let query = url::form_urlencoded::Serializer::new(String::new())
                .append_pair(key, value)
                .finish();
            url.push('?');
            url.push_str(&query);
        }
        url
    }

    pub async fn handle_upload(
        &self,
        token: &str,
        body: Body,
        max_bytes: usize,
    ) -> anyhow::Result<LocalUploadOutcome> {
        let token_hash = hash_token(token);
        let now = now_unix();
        let entry = {
            let mut guard = self.upload_tokens.lock().await;
            guard.remove(&token_hash, now)
        };

        let Some(entry) = entry else {
            return Ok(LocalUploadOutcome::InvalidToken);
        };

        let path = self.full_path_for_key(&entry.key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.with_context(|| {
                format!("failed to create parent directory: {}", parent.display())
            })?;
        }

        let temp_path = path.with_extension(format!("oore-upload-{}.tmp", Uuid::new_v4()));
        let file = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .await
            .with_context(|| format!("failed to create upload file: {}", temp_path.display()))?;
        let result = Self::write_upload(file, &temp_path, &path, body, max_bytes).await;
        if !matches!(result, Ok(LocalUploadOutcome::Stored)) {
            match tokio::fs::remove_file(&temp_path).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(cleanup_error) => {
                    return match result {
                        Err(error) => Err(error.context(format!(
                            "failed to remove partial upload {}: {cleanup_error}",
                            temp_path.display()
                        ))),
                        Ok(_) => Err(cleanup_error).with_context(|| {
                            format!("failed to remove partial upload: {}", temp_path.display())
                        }),
                    };
                }
            }
        }

        result
    }

    async fn write_upload(
        mut file: tokio::fs::File,
        temp_path: &Path,
        path: &Path,
        body: Body,
        max_bytes: usize,
    ) -> anyhow::Result<LocalUploadOutcome> {
        let mut bytes_read = 0usize;
        let mut stream = body.into_data_stream();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("failed to read artifact upload body")?;
            bytes_read = match bytes_read.checked_add(chunk.len()) {
                Some(total) if total <= max_bytes => total,
                _ => return Ok(LocalUploadOutcome::TooLarge),
            };
            file.write_all(&chunk)
                .await
                .with_context(|| format!("failed to write upload file: {}", temp_path.display()))?;
        }

        file.flush()
            .await
            .with_context(|| format!("failed to flush upload file: {}", temp_path.display()))?;
        drop(file);
        tokio::fs::rename(temp_path, path).await.with_context(|| {
            format!(
                "failed to publish artifact {} from {}",
                path.display(),
                temp_path.display()
            )
        })?;

        Ok(LocalUploadOutcome::Stored)
    }

    pub async fn handle_download(
        &self,
        token: &str,
    ) -> anyhow::Result<Option<LocalDownloadPayload>> {
        let token_hash = hash_token(token);
        let now = now_unix();

        let entry = {
            let mut guard = self.download_tokens.lock().await;
            guard.get(&token_hash, now)
        };

        let Some(entry) = entry else {
            return Ok(None);
        };

        let path = self.full_path_for_key(&entry.key)?;
        let permit = self
            .download_slots
            .clone()
            .acquire_owned()
            .await
            .context("local artifact download limiter closed")?;
        let file = match tokio::fs::File::open(&path).await {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(e).with_context(|| format!("failed to open {}", path.display()));
            }
        };

        let bytes = Body::from_stream(async_stream::stream! {
            let _permit = permit;
            let mut file = file;
            loop {
                let mut chunk = vec![0_u8; LOCAL_DOWNLOAD_CHUNK_BYTES];
                match file.read(&mut chunk).await {
                    Ok(0) => break,
                    Ok(read) => {
                        chunk.truncate(read);
                        yield Ok::<Bytes, std::io::Error>(Bytes::from(chunk));
                    }
                    Err(error) => {
                        yield Err(error);
                        break;
                    }
                }
            }
        });

        let file_name = Path::new(&entry.key)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("artifact.bin")
            .to_string();

        Ok(Some(LocalDownloadPayload { bytes, file_name }))
    }

    pub async fn delete_file(&self, key: &str) -> Result<(), anyhow::Error> {
        let path = self.full_path_for_key(key)?;
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e)
                .with_context(|| format!("failed to delete local artifact: {}", path.display())),
        }
    }

    pub fn base_dir(&self) -> String {
        self.base_dir.to_string_lossy().to_string()
    }
}

pub struct LocalDownloadPayload {
    pub bytes: Body,
    pub file_name: String,
}

#[tokio::test]
async fn local_download_links_preserve_origin_and_token_checks() {
    let directory = std::env::temp_dir().join(format!("oore-download-{}", uuid::Uuid::new_v4()));
    let client = LocalStorageClient::new(directory.clone(), None).unwrap();
    std::fs::write(directory.join("app.apk"), b"apk content").unwrap();
    let direct = client.generate_download_url("app.apk", 60).await;
    let install = client
        .generate_download_url_with_base("app.apk", 60, None, None)
        .await;
    assert!(direct.starts_with("/v1/artifacts/download/"));
    assert!(install.starts_with("/install/download/"));
    for link in [&direct, &install] {
        let token = link.rsplit('/').next().unwrap();
        let payload = client.handle_download(token).await.unwrap().unwrap();
        assert_eq!(
            axum::body::to_bytes(payload.bytes, 100)
                .await
                .unwrap()
                .as_ref(),
            b"apk content"
        );
    }
    let remote = client
        .generate_download_url_with_base(
            "app.apk",
            60,
            Some("https://files.example.com"),
            Some(("warpgate-ticket", "ticket")),
        )
        .await;
    assert!(remote.starts_with("https://files.example.com/install/download/"));
    assert!(remote.ends_with("?warpgate-ticket=ticket"));
    let expired = client.generate_download_url("app.apk", 0).await;
    assert!(
        client
            .handle_download(expired.rsplit('/').next().unwrap())
            .await
            .unwrap()
            .is_none()
    );
    assert!(client.handle_download("invalid").await.unwrap().is_none());
    std::fs::remove_dir_all(directory).unwrap();
}

#[derive(Clone)]
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

    pub async fn generate_download_url_with_base(
        &self,
        key: &str,
        ttl_secs: u64,
        public_base_url: Option<&str>,
        query_pair: Option<(&str, &str)>,
    ) -> Result<Option<String>, anyhow::Error> {
        match self {
            Self::Disabled => Ok(None),
            Self::S3(client) => client.generate_download_url(key, ttl_secs).await.map(Some),
            Self::Local(client) => Ok(Some(
                client
                    .generate_download_url_with_base(key, ttl_secs, public_base_url, query_pair)
                    .await,
            )),
        }
    }

    pub fn local_client(&self) -> Option<Arc<LocalStorageClient>> {
        match self {
            Self::Local(client) => Some(Arc::clone(client)),
            _ => None,
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

    pub async fn delete_object(&self, key: &str) -> Result<(), anyhow::Error> {
        match self {
            Self::Disabled => Ok(()),
            Self::S3(client) => client.delete_object(key).await,
            Self::Local(client) => client.delete_file(key).await,
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
    match resolve_oored_data_dir() {
        Ok(root) => root.join("artifacts"),
        Err(_) => {
            let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
            base.join("oore").join("artifacts")
        }
    }
}

pub fn build_backend_from_config(
    config: &EffectiveStorageConfig,
    public_base_url: Option<String>,
) -> anyhow::Result<StorageBackend> {
    match config.provider {
        ArtifactStorageProvider::Disabled => Ok(StorageBackend::Disabled),
        ArtifactStorageProvider::Local => {
            let base_dir = config
                .local_base_dir
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(default_local_artifacts_dir);
            let client = LocalStorageClient::new(base_dir, public_base_url)?;
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

            Ok(StorageBackend::S3(StorageClient::new(storage_config)?))
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
        let provider =
            ArtifactStorageProvider::from_str(&provider_str).map_err(|e| anyhow::anyhow!(e))?;

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
        provider: ArtifactStorageProvider::Local,
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
    public_base_url: Option<String>,
) -> StorageBackend {
    match load_effective_config(pool, encryption_key).await {
        Ok(cfg) => match build_backend_from_config(&cfg, public_base_url) {
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
