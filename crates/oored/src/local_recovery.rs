use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::time::Duration;

use anyhow::{Context, bail};
use oore_contract::{
    ApiError, LOCAL_RECOVERY_MAX_TTL_SECS, LOCAL_RECOVERY_MIN_TTL_SECS, LOCAL_RECOVERY_SOCKET_DIR,
    LOCAL_RECOVERY_SOCKET_FILE, LocalRecoveryMintRequest, LocalRecoveryMintResponse, RuntimeMode,
    SetupState,
};
use sqlx::{Row, SqlitePool};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;
use tracing::{error, warn};
use uuid::Uuid;

use crate::instance_settings::load_runtime_mode;
use crate::store::write_audit_log;
use crate::token::{generate_token, hash_token};
use crate::util::now_unix;

const CAPABILITY_PREFIX: &str = "oore_recovery_";
const MAX_CAPABILITIES: usize = 32;
const MAX_MANAGEMENT_REQUEST_BYTES: u64 = 4096;
const MANAGEMENT_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Default)]
pub struct RecoveryCapabilityStore {
    inner: Arc<Mutex<HashMap<String, RecoveryCapability>>>,
}

struct RecoveryCapability {
    id: String,
    user_id: String,
    user_email: String,
    expires_at: i64,
}

pub struct ConsumedRecoveryCapability {
    pub id: String,
    pub user_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumeError {
    Malformed,
    UnknownOrExpired,
    AccountMismatch,
}

impl ConsumeError {
    pub fn reason(self) -> &'static str {
        match self {
            Self::Malformed => "malformed",
            Self::UnknownOrExpired => "unknown_or_expired",
            Self::AccountMismatch => "account_mismatch",
        }
    }
}

impl RecoveryCapabilityStore {
    pub async fn mint(
        &self,
        user_id: String,
        user_email: String,
        ttl_seconds: u64,
    ) -> anyhow::Result<(String, String, i64)> {
        if !(LOCAL_RECOVERY_MIN_TTL_SECS..=LOCAL_RECOVERY_MAX_TTL_SECS).contains(&ttl_seconds) {
            bail!(
                "recovery capability TTL must be between {LOCAL_RECOVERY_MIN_TTL_SECS} and {LOCAL_RECOVERY_MAX_TTL_SECS} seconds"
            );
        }

        let now = now_unix();
        let expires_at = now + i64::try_from(ttl_seconds).unwrap_or(i64::MAX);
        let raw = format!("{CAPABILITY_PREFIX}{}", generate_token());
        let token_hash = hash_token(&raw);
        let id = Uuid::new_v4().to_string();

        {
            let mut capabilities = self.inner.lock().await;
            capabilities.retain(|_, value| value.expires_at > now);
            if capabilities.len() >= MAX_CAPABILITIES {
                bail!("too many active recovery capabilities");
            }
            capabilities.insert(
                token_hash.clone(),
                RecoveryCapability {
                    id: id.clone(),
                    user_id,
                    user_email,
                    expires_at,
                },
            );
        }

        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(expire_capability(weak, token_hash, ttl_seconds));
        Ok((raw, id, expires_at))
    }

    pub async fn consume(
        &self,
        raw: &str,
        requested_email: Option<&str>,
    ) -> Result<ConsumedRecoveryCapability, ConsumeError> {
        if !valid_capability_format(raw) {
            return Err(ConsumeError::Malformed);
        }

        let now = now_unix();
        let mut capabilities = self.inner.lock().await;
        capabilities.retain(|_, value| value.expires_at > now);
        let capability = capabilities
            .remove(&hash_token(raw))
            .ok_or(ConsumeError::UnknownOrExpired)?;

        if requested_email.is_some_and(|email| {
            !email
                .trim()
                .eq_ignore_ascii_case(capability.user_email.as_str())
        }) {
            return Err(ConsumeError::AccountMismatch);
        }

        Ok(ConsumedRecoveryCapability {
            id: capability.id,
            user_id: capability.user_id,
        })
    }

    pub async fn clear(&self) {
        self.inner.lock().await.clear();
    }
}

async fn expire_capability(
    weak: Weak<Mutex<HashMap<String, RecoveryCapability>>>,
    token_hash: String,
    ttl_seconds: u64,
) {
    tokio::time::sleep(Duration::from_secs(ttl_seconds)).await;
    if let Some(inner) = weak.upgrade() {
        inner.lock().await.remove(&token_hash);
    }
}

fn valid_capability_format(raw: &str) -> bool {
    raw.len() == CAPABILITY_PREFIX.len() + 64
        && raw.starts_with(CAPABILITY_PREFIX)
        && raw[CAPABILITY_PREFIX.len()..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
}

pub fn management_socket_path(database_path: &Path) -> PathBuf {
    let parent = database_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    parent
        .join(LOCAL_RECOVERY_SOCKET_DIR)
        .join(LOCAL_RECOVERY_SOCKET_FILE)
}

pub struct ManagementSocket {
    listener: UnixListener,
    path: PathBuf,
    pool: SqlitePool,
    capabilities: RecoveryCapabilityStore,
    expected_uid: u32,
}

impl ManagementSocket {
    pub async fn bind(
        path: PathBuf,
        pool: SqlitePool,
        capabilities: RecoveryCapabilityStore,
    ) -> anyhow::Result<Self> {
        // SAFETY: geteuid has no preconditions and does not dereference memory.
        let expected_uid = unsafe { libc::geteuid() };
        Self::bind_for_uid(path, pool, capabilities, expected_uid).await
    }

    async fn bind_for_uid(
        path: PathBuf,
        pool: SqlitePool,
        capabilities: RecoveryCapabilityStore,
        expected_uid: u32,
    ) -> anyhow::Result<Self> {
        let parent = path
            .parent()
            .context("management socket path has no parent directory")?;
        prepare_private_directory(parent, expected_uid)?;
        remove_owned_stale_socket(&path, expected_uid).await?;

        let listener = UnixListener::bind(&path)
            .with_context(|| format!("failed to bind management socket {}", path.display()))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).with_context(|| {
            format!(
                "failed to restrict management socket permissions {}",
                path.display()
            )
        })?;
        validate_socket(&path, expected_uid)?;

        Ok(Self {
            listener,
            path,
            pool,
            capabilities,
            expected_uid,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub async fn serve(self) -> anyhow::Result<()> {
        loop {
            let (stream, _) = self.listener.accept().await.with_context(|| {
                format!(
                    "failed to accept management socket connection on {}",
                    self.path.display()
                )
            })?;
            let pool = self.pool.clone();
            let capabilities = self.capabilities.clone();
            tokio::spawn(async move {
                if let Err(error) = handle_connection(stream, pool, capabilities).await {
                    warn!(%error, "local recovery management request failed");
                }
            });
        }
    }
}

impl Drop for ManagementSocket {
    fn drop(&mut self) {
        if validate_socket(&self.path, self.expected_uid).is_ok() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn prepare_private_directory(path: &Path, expected_uid: u32) -> anyhow::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new()
                .mode(0o700)
                .create(path)
                .with_context(|| {
                    format!("failed to create management directory {}", path.display())
                })?;
        }
        Err(error) => {
            return Err(error).with_context(|| {
                format!("failed to inspect management directory {}", path.display())
            });
        }
    }

    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect management directory {}", path.display()))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != expected_uid
        || metadata.mode() & 0o7777 != 0o700
    {
        bail!(
            "management directory {} must be a non-symlink directory owned by uid {} with mode 0700",
            path.display(),
            expected_uid
        );
    }
    Ok(())
}

async fn remove_owned_stale_socket(path: &Path, expected_uid: u32) -> anyhow::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => validate_socket(path, expected_uid)?,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect socket path {}", path.display()));
        }
    }

    match tokio::time::timeout(Duration::from_secs(1), UnixStream::connect(path)).await {
        Ok(Ok(_)) => bail!("management socket {} is already active", path.display()),
        Ok(Err(error))
            if matches!(
                error.kind(),
                ErrorKind::ConnectionRefused | ErrorKind::NotFound
            ) => {}
        Ok(Err(error)) => {
            return Err(error).with_context(|| {
                format!(
                    "refusing to replace unverifiable management socket {}",
                    path.display()
                )
            });
        }
        Err(_) => bail!(
            "timed out while verifying management socket {}; refusing to replace it",
            path.display()
        ),
    }

    fs::remove_file(path)
        .with_context(|| format!("failed to remove stale socket {}", path.display()))
}

fn validate_socket(path: &Path, expected_uid: u32) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect management socket {}", path.display()))?;
    if metadata.file_type().is_symlink()
        || !metadata.file_type().is_socket()
        || metadata.uid() != expected_uid
        || metadata.mode() & 0o7777 != 0o600
    {
        bail!(
            "management socket {} must be a non-symlink socket owned by uid {} with mode 0600",
            path.display(),
            expected_uid
        );
    }
    Ok(())
}

async fn handle_connection(
    mut stream: UnixStream,
    pool: SqlitePool,
    capabilities: RecoveryCapabilityStore,
) -> anyhow::Result<()> {
    let mut request = String::new();
    let read = tokio::time::timeout(MANAGEMENT_REQUEST_TIMEOUT, async {
        BufReader::new(&mut stream)
            .take(MAX_MANAGEMENT_REQUEST_BYTES + 1)
            .read_line(&mut request)
            .await
    })
    .await
    .context("management request timed out")??;

    let response = if read == 0
        || request.len() as u64 > MAX_MANAGEMENT_REQUEST_BYTES
        || !request.ends_with('\n')
    {
        audit_mint_failure(&pool, "invalid_request").await;
        mint_error("invalid_request", "Management request is invalid")
    } else {
        match serde_json::from_str::<LocalRecoveryMintRequest>(&request) {
            Ok(request) => mint_capability(&pool, &capabilities, request).await,
            Err(_) => {
                audit_mint_failure(&pool, "invalid_request").await;
                mint_error("invalid_request", "Management request is invalid")
            }
        }
    };

    let mut encoded = serde_json::to_vec(&response).context("failed to encode response")?;
    encoded.push(b'\n');
    stream
        .write_all(&encoded)
        .await
        .context("failed to write management response")?;
    Ok(())
}

async fn mint_capability(
    pool: &SqlitePool,
    capabilities: &RecoveryCapabilityStore,
    request: LocalRecoveryMintRequest,
) -> LocalRecoveryMintResponse {
    match try_mint_capability(pool, capabilities, request).await {
        Ok((response, capability_id, user_id, expires_at)) => {
            let details = serde_json::json!({
                "capability_id": capability_id,
                "expires_at": expires_at,
                "channel": "unix_socket",
            })
            .to_string();
            if let Err(error) = write_audit_log(
                pool,
                Some(&user_id),
                "local_recovery_capability_minted",
                "local_recovery_capability",
                Some(&capability_id),
                Some(&details),
            )
            .await
            {
                error!(%error, "failed to audit local recovery capability mint");
                capabilities.clear().await;
                return mint_error("audit_error", "Failed to mint recovery capability");
            }
            response
        }
        Err((code, message)) => {
            audit_mint_failure(pool, code).await;
            mint_error(code, message)
        }
    }
}

async fn audit_mint_failure(pool: &SqlitePool, reason: &str) {
    let details = serde_json::json!({ "reason": reason }).to_string();
    let _ = write_audit_log(
        pool,
        None,
        "local_recovery_capability_mint_failed",
        "local_recovery_capability",
        None,
        Some(&details),
    )
    .await;
}

async fn try_mint_capability(
    pool: &SqlitePool,
    capabilities: &RecoveryCapabilityStore,
    request: LocalRecoveryMintRequest,
) -> Result<(LocalRecoveryMintResponse, String, String, i64), (&'static str, &'static str)> {
    if !(LOCAL_RECOVERY_MIN_TTL_SECS..=LOCAL_RECOVERY_MAX_TTL_SECS).contains(&request.ttl_seconds) {
        return Err((
            "invalid_ttl",
            "Recovery capability TTL must be between 1 second and 5 minutes",
        ));
    }

    let setup_state: String =
        sqlx::query_scalar("SELECT setup_state FROM setup_state WHERE id = 1")
            .fetch_one(pool)
            .await
            .map_err(|_| ("store_error", "Failed to load setup state"))?;
    if setup_state != SetupState::Ready.to_string() {
        return Err((
            "setup_incomplete",
            "Recovery capabilities require a ready instance",
        ));
    }

    let runtime_mode = load_runtime_mode(pool)
        .await
        .map_err(|_| ("store_error", "Failed to load runtime mode"))?;
    if runtime_mode != RuntimeMode::Remote {
        return Err((
            "mode_restricted",
            "Recovery capabilities are only available in External Access mode",
        ));
    }

    let requested_email = request
        .email
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty());
    if requested_email
        .as_ref()
        .is_some_and(|email| email.len() > 256 || !email.contains('@'))
    {
        return Err(("invalid_email", "Recovery account email is invalid"));
    }

    let row = if let Some(email) = requested_email {
        sqlx::query(
            "SELECT id, email FROM users WHERE lower(email) = ?1 AND status = 'active' LIMIT 1",
        )
        .bind(email)
        .fetch_optional(pool)
        .await
        .map_err(|_| ("store_error", "Failed to look up recovery account"))?
    } else {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE status = 'active'")
            .fetch_one(pool)
            .await
            .map_err(|_| ("store_error", "Failed to look up recovery accounts"))?;
        if count != 1 {
            return Err((
                "email_required",
                "Specify --email when multiple active users exist",
            ));
        }
        sqlx::query("SELECT id, email FROM users WHERE status = 'active' LIMIT 1")
            .fetch_optional(pool)
            .await
            .map_err(|_| ("store_error", "Failed to look up recovery account"))?
    }
    .ok_or((
        "user_not_found",
        "No active recovery account matched the request",
    ))?;

    let user_id: String = row.get("id");
    let user_email: String = row.get("email");
    let (capability, capability_id, expires_at) = capabilities
        .mint(user_id.clone(), user_email.clone(), request.ttl_seconds)
        .await
        .map_err(|_| {
            (
                "recovery_capacity_exhausted",
                "Too many active recovery capabilities; wait for one to expire",
            )
        })?;

    Ok((
        LocalRecoveryMintResponse::Success {
            capability,
            expires_at,
            user_email,
        },
        capability_id,
        user_id,
        expires_at,
    ))
}

fn mint_error(code: &str, message: &str) -> LocalRecoveryMintResponse {
    LocalRecoveryMintResponse::Error {
        error: ApiError::new(code, message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn capability_is_single_use_and_account_bound() {
        let store = RecoveryCapabilityStore::default();
        let (raw, _, _) = store
            .mint("user-1".to_string(), "owner@example.com".to_string(), 60)
            .await
            .expect("mint");
        assert!(matches!(
            store.consume(&raw, Some("other@example.com")).await,
            Err(ConsumeError::AccountMismatch)
        ));
        assert!(matches!(
            store.consume(&raw, None).await,
            Err(ConsumeError::UnknownOrExpired)
        ));
    }

    #[tokio::test]
    async fn capability_store_is_bounded_and_clear_revokes_everything() {
        let store = RecoveryCapabilityStore::default();
        let mut issued = Vec::new();
        for _ in 0..MAX_CAPABILITIES {
            issued.push(
                store
                    .mint("user-1".to_string(), "owner@example.com".to_string(), 60)
                    .await
                    .expect("mint within capacity")
                    .0,
            );
        }
        assert!(
            store
                .mint("user-1".to_string(), "owner@example.com".to_string(), 60)
                .await
                .is_err()
        );

        store.clear().await;
        assert!(matches!(
            store.consume(&issued[0], None).await,
            Err(ConsumeError::UnknownOrExpired)
        ));
    }

    #[tokio::test]
    async fn socket_path_hazards_fail_closed_and_stale_socket_is_replaced() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let db = tmp.path().join("test.db");
        let pool = SqlitePool::connect(":memory:").await.expect("pool");
        // SAFETY: geteuid has no preconditions and does not dereference memory.
        let uid = unsafe { libc::geteuid() };

        let bad_dir = tmp.path().join("bad-run");
        fs::create_dir(&bad_dir).expect("bad dir");
        fs::set_permissions(&bad_dir, fs::Permissions::from_mode(0o755)).expect("bad mode");
        let bad_path = bad_dir.join(LOCAL_RECOVERY_SOCKET_FILE);
        assert!(
            ManagementSocket::bind_for_uid(
                bad_path,
                pool.clone(),
                RecoveryCapabilityStore::default(),
                uid,
            )
            .await
            .is_err()
        );

        let socket_path = management_socket_path(&db);
        prepare_private_directory(socket_path.parent().expect("parent"), uid).expect("private dir");
        let stale = std::os::unix::net::UnixListener::bind(&socket_path).expect("stale socket");
        fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600)).expect("socket mode");
        drop(stale);
        let server = ManagementSocket::bind_for_uid(
            socket_path.clone(),
            pool.clone(),
            RecoveryCapabilityStore::default(),
            uid,
        )
        .await
        .expect("replace stale socket");
        assert!(socket_path.exists());
        drop(server);

        let active = std::os::unix::net::UnixListener::bind(&socket_path).expect("active socket");
        fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600)).expect("socket mode");
        assert!(
            ManagementSocket::bind_for_uid(
                socket_path.clone(),
                pool.clone(),
                RecoveryCapabilityStore::default(),
                uid,
            )
            .await
            .is_err()
        );
        drop(active);
        fs::remove_file(&socket_path).expect("remove active socket path");

        let target = tmp.path().join("target");
        fs::write(&target, b"not a socket").expect("target");
        std::os::unix::fs::symlink(&target, &socket_path).expect("socket symlink");
        assert!(
            ManagementSocket::bind_for_uid(
                socket_path,
                pool,
                RecoveryCapabilityStore::default(),
                uid,
            )
            .await
            .is_err()
        );

        let real_parent = tmp.path().join("real-run");
        fs::create_dir(&real_parent).expect("real parent");
        fs::set_permissions(&real_parent, fs::Permissions::from_mode(0o700))
            .expect("real parent mode");
        let linked_parent = tmp.path().join("linked-run");
        std::os::unix::fs::symlink(&real_parent, &linked_parent).expect("parent symlink");
        assert!(
            ManagementSocket::bind_for_uid(
                linked_parent.join(LOCAL_RECOVERY_SOCKET_FILE),
                SqlitePool::connect(":memory:").await.expect("pool"),
                RecoveryCapabilityStore::default(),
                uid,
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn wrong_owner_is_rejected() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let path = tmp.path().join("run");
        fs::create_dir(&path).expect("dir");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).expect("mode");
        // SAFETY: geteuid has no preconditions and does not dereference memory.
        let uid = unsafe { libc::geteuid() };
        assert!(prepare_private_directory(&path, uid.saturating_add(1)).is_err());
    }
}
