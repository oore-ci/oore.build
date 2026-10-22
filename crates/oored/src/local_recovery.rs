use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::time::Duration;

use anyhow::{Context, bail};
use oore_contract::{
    ApiError, LOCAL_RECOVERY_MAX_TTL_SECS, LOCAL_RECOVERY_MIN_TTL_SECS, LocalRecoveryMintRequest,
    LocalRecoveryMintResponse, OperatorRequestEnvelope, OperatorResponse, RuntimeMode, SetupState,
    local_recovery_socket_path,
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

pub fn management_socket_path(database_path: &Path) -> anyhow::Result<PathBuf> {
    // SAFETY: geteuid has no preconditions and does not dereference memory.
    local_recovery_socket_path(database_path, unsafe { libc::geteuid() }).with_context(|| {
        format!(
            "failed to resolve management socket path for {}",
            database_path.display()
        )
    })
}

pub struct ManagementSocket {
    listener: UnixListener,
    path: PathBuf,
    pool: SqlitePool,
    capabilities: RecoveryCapabilityStore,
    expected_uid: u32,
    expected_gid: u32,
}

impl ManagementSocket {
    pub async fn bind(
        path: PathBuf,
        pool: SqlitePool,
        capabilities: RecoveryCapabilityStore,
    ) -> anyhow::Result<Self> {
        // SAFETY: geteuid has no preconditions and does not dereference memory.
        let expected_uid = unsafe { libc::geteuid() };
        // SAFETY: getegid has no preconditions and does not dereference memory.
        let expected_gid = unsafe { libc::getegid() };
        Self::bind_for_identity(path, pool, capabilities, expected_uid, expected_gid).await
    }

    async fn bind_for_identity(
        path: PathBuf,
        pool: SqlitePool,
        capabilities: RecoveryCapabilityStore,
        expected_uid: u32,
        expected_gid: u32,
    ) -> anyhow::Result<Self> {
        let parent = path
            .parent()
            .context("management socket path has no parent directory")?;
        prepare_private_directory_for_identity(parent, expected_uid, expected_gid)?;
        remove_owned_stale_socket(&path, expected_uid, expected_gid).await?;

        let listener = UnixListener::bind(&path)
            .with_context(|| format!("failed to bind management socket {}", path.display()))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).with_context(|| {
            format!(
                "failed to restrict management socket permissions {}",
                path.display()
            )
        })?;
        validate_socket(&path, expected_uid, expected_gid)?;

        Ok(Self {
            listener,
            path,
            pool,
            capabilities,
            expected_uid,
            expected_gid,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub async fn serve(self) -> anyhow::Result<()> {
        let pool = self.pool.clone();
        let capabilities = self.capabilities.clone();
        let expected_uid = self.expected_uid;
        let expected_gid = self.expected_gid;
        accept_connections(
            || self.listener.accept(),
            || validate_socket(&self.path, expected_uid, expected_gid),
            |stream| {
                let pool = pool.clone();
                let capabilities = capabilities.clone();
                tokio::spawn(async move {
                    if let Err(error) =
                        handle_connection(stream, pool, capabilities, expected_uid, expected_gid)
                            .await
                    {
                        warn!(%error, "local recovery management request failed");
                    }
                });
            },
        )
        .await
        .with_context(|| {
            format!(
                "failed to accept management socket connection on {}",
                self.path.display()
            )
        })
    }
}

const ACCEPT_RETRY_MIN: Duration = Duration::from_millis(100);
const ACCEPT_RETRY_MAX: Duration = Duration::from_secs(1);

/// Accept connections until the socket itself is gone. A failed `accept`, for
/// example when the daemon briefly runs out of file descriptors, only pauses
/// the loop, so local recovery keeps working while the daemon stays up.
async fn accept_connections<Accept, Fut, Stream>(
    mut accept: Accept,
    socket_is_usable: impl Fn() -> anyhow::Result<()>,
    mut handle: impl FnMut(Stream),
) -> anyhow::Result<()>
where
    Accept: FnMut() -> Fut,
    Fut: Future<Output = std::io::Result<(Stream, tokio::net::unix::SocketAddr)>>,
{
    let mut retry = ACCEPT_RETRY_MIN;
    loop {
        match accept().await {
            Ok((stream, _)) => {
                retry = ACCEPT_RETRY_MIN;
                handle(stream);
            }
            Err(error) => {
                if let Err(unusable) = socket_is_usable() {
                    return Err(anyhow::Error::from(error).context(unusable));
                }
                warn!(
                    %error,
                    retry_ms = retry.as_millis() as u64,
                    "local recovery management socket failed to accept a connection; retrying"
                );
                tokio::time::sleep(retry).await;
                retry = (retry * 2).min(ACCEPT_RETRY_MAX);
            }
        }
    }
}

impl Drop for ManagementSocket {
    fn drop(&mut self) {
        if validate_socket(&self.path, self.expected_uid, self.expected_gid).is_ok() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn prepare_private_directory_for_identity(
    path: &Path,
    expected_uid: u32,
    _expected_gid: u32,
) -> anyhow::Result<()> {
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

async fn remove_owned_stale_socket(
    path: &Path,
    expected_uid: u32,
    expected_gid: u32,
) -> anyhow::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => validate_socket(path, expected_uid, expected_gid)?,
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

fn validate_socket(path: &Path, expected_uid: u32, _expected_gid: u32) -> anyhow::Result<()> {
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
    expected_uid: u32,
    expected_gid: u32,
) -> anyhow::Result<()> {
    let peer = stream
        .peer_cred()
        .context("failed to inspect management socket peer")?;
    if !peer_is_authorized(peer.uid(), peer.gid(), expected_uid, expected_gid) {
        let mut encoded = serde_json::to_vec(&OperatorResponse::Error {
            code: "peer_not_authorized".to_string(),
            message: "Management socket peer is not authorized".to_string(),
        })?;
        encoded.push(b'\n');
        stream.write_all(&encoded).await?;
        return Ok(());
    }

    let mut request = String::new();
    let read = tokio::time::timeout(MANAGEMENT_REQUEST_TIMEOUT, async {
        BufReader::new(&mut stream)
            .take(MAX_MANAGEMENT_REQUEST_BYTES + 1)
            .read_line(&mut request)
            .await
    })
    .await
    .context("management request timed out")??;

    if read == 0 || request.len() as u64 > MAX_MANAGEMENT_REQUEST_BYTES || !request.ends_with('\n')
    {
        audit_mint_failure(&pool, "invalid_request").await;
        let mut encoded = serde_json::to_vec(&mint_error(
            "invalid_request",
            "Management request is invalid",
        ))?;
        encoded.push(b'\n');
        stream.write_all(&encoded).await?;
        return Ok(());
    }

    let parsed = serde_json::from_str::<serde_json::Value>(&request).ok();
    let is_operator_request = parsed.as_ref().is_some_and(|value| {
        value.get("request").is_some()
            || value.get("expected_instance_id").is_some()
            || value.get("operation").is_some()
    });
    if is_operator_request {
        let response = match parsed
            .and_then(|value| serde_json::from_value::<OperatorRequestEnvelope>(value).ok())
        {
            Some(envelope) => match crate::operator::handle_local(&pool, envelope).await {
                Ok(response) => response,
                Err(error) => {
                    error!(%error, "local operator action failed");
                    audit_operator_failure(&pool, "operator_failed").await;
                    OperatorResponse::Error {
                        code: "operator_failed".to_string(),
                        message: error.to_string(),
                    }
                }
            },
            None => {
                audit_operator_failure(&pool, "invalid_operator_request").await;
                OperatorResponse::Error {
                    code: "invalid_operator_request".to_string(),
                    message: "Management request is invalid".to_string(),
                }
            }
        };
        let mut encoded =
            serde_json::to_vec(&response).context("failed to encode operator response")?;
        encoded.push(b'\n');
        stream.write_all(&encoded).await?;
        return Ok(());
    }

    let response = match serde_json::from_str::<LocalRecoveryMintRequest>(&request) {
        Ok(request) => mint_capability(&pool, &capabilities, request).await,
        Err(_) => {
            audit_mint_failure(&pool, "invalid_request").await;
            mint_error("invalid_request", "Management request is invalid")
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

fn peer_is_authorized(
    peer_uid: u32,
    _peer_gid: u32,
    expected_uid: u32,
    _expected_gid: u32,
) -> bool {
    peer_uid == expected_uid
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

async fn audit_operator_failure(pool: &SqlitePool, reason: &str) {
    let details = serde_json::json!({ "reason": reason, "channel": "unix_socket" }).to_string();
    let _ = write_audit_log(
        pool,
        None,
        "local_operator_request_failed",
        "local_operator_request",
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
    use super::accept_connections;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::net::{UnixListener, UnixStream};

    #[tokio::test]
    async fn a_failed_accept_does_not_stop_the_socket() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("management.sock");
        let listener = Arc::new(UnixListener::bind(&path).unwrap());
        let attempts = Arc::new(AtomicUsize::new(0));
        let (accepted, mut connections) = tokio::sync::mpsc::unbounded_channel();
        let server = tokio::spawn({
            let listener = listener.clone();
            let attempts = attempts.clone();
            async move {
                accept_connections(
                    move || {
                        let listener = listener.clone();
                        let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                        async move {
                            if attempt == 0 {
                                return Err(std::io::Error::from_raw_os_error(libc::EMFILE));
                            }
                            listener.accept().await
                        }
                    },
                    || Ok(()),
                    move |stream: UnixStream| accepted.send(stream).unwrap(),
                )
                .await
            }
        });

        let _client = UnixStream::connect(&path).await.unwrap();
        let connection = tokio::time::timeout(Duration::from_secs(5), connections.recv())
            .await
            .expect("the socket accepted a connection after the error");

        assert!(connection.is_some());
        assert!(attempts.load(Ordering::SeqCst) >= 2);
        assert!(!server.is_finished());
        server.abort();
    }

    #[tokio::test]
    async fn an_accept_error_on_a_removed_socket_stops_the_loop() {
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            accept_connections(
                || async {
                    Err::<(UnixStream, tokio::net::unix::SocketAddr), _>(
                        std::io::Error::from_raw_os_error(libc::EBADF),
                    )
                },
                || anyhow::bail!("management socket was removed"),
                |_stream: UnixStream| {},
            ),
        )
        .await
        .expect("the loop ended");

        assert!(format!("{:#}", result.unwrap_err()).contains("management socket was removed"));
    }
}
