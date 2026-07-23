use std::path::{Path, PathBuf};
use std::{env, fs};

use anyhow::{Context, bail};
use oore_contract::{
    BootstrapTokenRecord, OidcConfigRecord, OidcSecretRecord, OwnerRecord, SetupSessionRecord,
    SetupState, SetupStateFile,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use tracing::info;
use uuid::Uuid;

use crate::util::{now_unix, resolve_oored_data_dir};

fn local_subject_for_email(email: &str) -> String {
    format!("local::{}", email.trim().to_lowercase())
}

/// SQLite-backed state store for the setup state machine.
pub struct SetupStore {
    pool: SqlitePool,
    path: PathBuf,
}

impl SetupStore {
    /// Create a new store by connecting to the SQLite database at `path`.
    pub async fn connect(path: PathBuf) -> anyhow::Result<Self> {
        let path = if path.is_absolute() {
            path
        } else {
            env::current_dir()
                .context("failed to resolve the current directory")?
                .join(path)
        };

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory: {}", parent.display()))?;
        }

        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .with_context(|| format!("failed to connect to SQLite database: {}", path.display()))?;

        // Run embedded migrations
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .context("failed to run database migrations")?;

        Ok(Self { pool, path })
    }

    /// Resolve the database path from (in priority order):
    /// 1. Explicit `override_path` argument
    /// 2. `OORE_SETUP_STATE_FILE` env var
    /// 3. Default: `<data_root>/oore.db` where data root is:
    ///    `OORED_DATA_DIR` -> `OORE_DATA_DIR` -> `dirs::data_dir()/oore`
    pub fn resolve_path(override_path: Option<&str>) -> anyhow::Result<PathBuf> {
        if let Some(p) = override_path {
            return Ok(PathBuf::from(p));
        }

        if let Ok(p) = env::var("OORE_SETUP_STATE_FILE") {
            return Ok(PathBuf::from(p));
        }

        Ok(resolve_oored_data_dir()?.join("oore.db"))
    }

    /// Load the setup state from the database.
    pub async fn load(&self) -> anyhow::Result<SetupStateFile> {
        let row = sqlx::query("SELECT * FROM setup_state WHERE id = 1")
            .fetch_optional(&self.pool)
            .await
            .context("failed to query setup_state")?;

        let row = row.context("no setup state row found — call init_if_missing first")?;

        row_to_state_file(&row)
    }

    /// Save the setup state to the database (INSERT OR REPLACE, transactional).
    pub async fn save(&self, state: &SetupStateFile) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO setup_state (
                id, schema_version, instance_id, setup_state,
                bootstrap_token_hash, bootstrap_token_expires_at, bootstrap_token_consumed_at,
                session_hash, session_expires_at,
                oidc_issuer_url, oidc_client_id, oidc_has_client_secret,
                oidc_authorization_endpoint, oidc_token_endpoint,
                oidc_userinfo_endpoint, oidc_jwks_uri, oidc_configured_at,
                oidc_encrypted_client_secret, oidc_secret_stored_at,
                owner_email, owner_oidc_subject, owner_created_at,
                created_at, updated_at
            ) VALUES (
                1, ?1, ?2, ?3,
                ?4, ?5, ?6,
                ?7, ?8,
                ?9, ?10, ?11,
                ?12, ?13,
                ?14, ?15, ?16,
                ?17, ?18,
                ?19, ?20, ?21,
                ?22, ?23
            )
            "#,
        )
        .bind(state.schema_version as i64)
        .bind(&state.instance_id)
        .bind(setup_state_to_str(state.setup_state))
        // Bootstrap token
        .bind(state.bootstrap_token.as_ref().map(|t| &t.hash))
        .bind(state.bootstrap_token.as_ref().map(|t| t.expires_at))
        .bind(state.bootstrap_token.as_ref().and_then(|t| t.consumed_at))
        // Session
        .bind(state.setup_session.as_ref().map(|s| &s.hash))
        .bind(state.setup_session.as_ref().map(|s| s.expires_at))
        // OIDC config
        .bind(state.oidc_config.as_ref().map(|c| &c.issuer_url))
        .bind(state.oidc_config.as_ref().map(|c| &c.client_id))
        .bind(
            state
                .oidc_config
                .as_ref()
                .map(|c| c.has_client_secret as i32),
        )
        .bind(
            state
                .oidc_config
                .as_ref()
                .map(|c| &c.authorization_endpoint),
        )
        .bind(state.oidc_config.as_ref().map(|c| &c.token_endpoint))
        .bind(
            state
                .oidc_config
                .as_ref()
                .and_then(|c| c.userinfo_endpoint.as_ref()),
        )
        .bind(state.oidc_config.as_ref().map(|c| &c.jwks_uri))
        .bind(state.oidc_config.as_ref().map(|c| c.configured_at))
        // OIDC secret
        .bind(
            state
                .oidc_secret
                .as_ref()
                .map(|s| &s.encrypted_client_secret),
        )
        .bind(state.oidc_secret.as_ref().map(|s| s.stored_at))
        // Owner
        .bind(state.owner.as_ref().map(|o| &o.email))
        .bind(state.owner.as_ref().and_then(|o| o.oidc_subject.as_ref()))
        .bind(state.owner.as_ref().map(|o| o.created_at))
        // Timestamps
        .bind(state.created_at)
        .bind(state.updated_at)
        .execute(&self.pool)
        .await
        .context("failed to save setup state")?;

        Ok(())
    }

    /// Return a reference to the underlying connection pool.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Return the absolute path backing this store.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Ensure the owner user row exists in the `users` table.
    ///
    /// Called on startup after migrations. If `setup_state == Ready` and the
    /// `users` table has zero rows, backfill the owner from the setup state
    /// file. This handles existing instances upgraded from before migration 002.
    pub async fn ensure_owner_user(&self) -> anyhow::Result<()> {
        let sf = self.load().await?;
        if sf.setup_state != SetupState::Ready {
            return Ok(());
        }

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await
            .context("failed to count users")?;

        if count > 0 {
            return Ok(());
        }

        let owner = sf
            .owner
            .as_ref()
            .context("setup is Ready but no owner record exists")?;
        let oidc_subject = owner
            .oidc_subject
            .clone()
            .unwrap_or_else(|| local_subject_for_email(&owner.email));

        let user_id = Uuid::new_v4().to_string();
        let now = now_unix();

        sqlx::query(
            "INSERT INTO users (id, email, oidc_subject, display_name, role, status, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, 'owner', 'active', ?5, ?5)",
        )
        .bind(&user_id)
        .bind(&owner.email)
        .bind(&oidc_subject)
        .bind(&owner.email)
        .bind(now)
        .execute(&self.pool)
        .await
        .context("failed to backfill owner user")?;

        // Audit log
        write_audit_log(
            &self.pool,
            Some(&user_id),
            "owner_backfilled",
            "user",
            Some(&user_id),
            None,
        )
        .await?;

        info!(email = %owner.email, "backfilled owner user from setup state");
        Ok(())
    }

    /// If no setup state row exists, create one with initial
    /// `BootstrapPending` state, a fresh UUID instance id, and the current
    /// timestamp. If it already exists, just load and return it.
    pub async fn init_if_missing(&self) -> anyhow::Result<SetupStateFile> {
        let existing = sqlx::query("SELECT id FROM setup_state WHERE id = 1")
            .fetch_optional(&self.pool)
            .await
            .context("failed to check for existing setup state")?;

        if existing.is_some() {
            return self.load().await;
        }

        let now = now_unix();
        let state = SetupStateFile {
            schema_version: SetupStateFile::CURRENT_SCHEMA_VERSION,
            instance_id: Uuid::new_v4().to_string(),
            setup_state: SetupState::BootstrapPending,
            bootstrap_token: None,
            setup_session: None,
            oidc_config: None,
            oidc_secret: None,
            owner: None,
            created_at: now,
            updated_at: now,
        };

        self.save(&state).await?;

        // Verify the round-trip so we fail loudly on serialization bugs.
        let loaded = self
            .load()
            .await
            .context("failed to reload newly created state")?;
        if loaded.instance_id != state.instance_id {
            bail!("state round-trip verification failed");
        }

        Ok(state)
    }
}

// ── Conversion helpers ──────────────────────────────────────────

fn setup_state_to_str(state: SetupState) -> &'static str {
    match state {
        SetupState::Uninitialized => "uninitialized",
        SetupState::BootstrapPending => "bootstrap_pending",
        SetupState::IdpConfigured => "idp_configured",
        SetupState::OwnerCreated => "owner_created",
        SetupState::Ready => "ready",
        _ => "unknown",
    }
}

fn str_to_setup_state(s: &str) -> anyhow::Result<SetupState> {
    match s {
        "uninitialized" => Ok(SetupState::Uninitialized),
        "bootstrap_pending" => Ok(SetupState::BootstrapPending),
        "idp_configured" => Ok(SetupState::IdpConfigured),
        "owner_created" => Ok(SetupState::OwnerCreated),
        "ready" => Ok(SetupState::Ready),
        other => bail!("unknown setup state: {other}"),
    }
}

/// Write an entry to the audit_logs table.
pub async fn write_audit_log(
    pool: &SqlitePool,
    actor_id: Option<&str>,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    details: Option<&str>,
) -> anyhow::Result<()> {
    let now = now_unix();
    sqlx::query(
        "INSERT INTO audit_logs (actor_id, action, resource_type, resource_id, details, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(actor_id)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(details)
    .bind(now)
    .execute(pool)
    .await
    .context("failed to write audit log")?;
    Ok(())
}

fn row_to_state_file(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<SetupStateFile> {
    let schema_version: i64 = row.try_get("schema_version")?;
    let instance_id: String = row.try_get("instance_id")?;
    let state_str: String = row.try_get("setup_state")?;
    let setup_state = str_to_setup_state(&state_str)?;

    // Bootstrap token
    let bootstrap_token = {
        let hash: Option<String> = row.try_get("bootstrap_token_hash")?;
        hash.map(|hash| {
            let expires_at: i64 = row.try_get("bootstrap_token_expires_at").unwrap_or(0);
            let consumed_at: Option<i64> =
                row.try_get("bootstrap_token_consumed_at").unwrap_or(None);
            BootstrapTokenRecord {
                hash,
                expires_at,
                consumed_at,
            }
        })
    };

    // Setup session
    let setup_session = {
        let hash: Option<String> = row.try_get("session_hash")?;
        hash.map(|hash| {
            let expires_at: i64 = row.try_get("session_expires_at").unwrap_or(0);
            SetupSessionRecord { hash, expires_at }
        })
    };

    // OIDC config
    let oidc_config = {
        let issuer_url: Option<String> = row.try_get("oidc_issuer_url")?;
        issuer_url.map(|issuer_url| {
            let client_id: String = row.try_get("oidc_client_id").unwrap_or_default();
            let has_client_secret: i32 = row.try_get("oidc_has_client_secret").unwrap_or(0);
            let authorization_endpoint: String = row
                .try_get("oidc_authorization_endpoint")
                .unwrap_or_default();
            let token_endpoint: String = row.try_get("oidc_token_endpoint").unwrap_or_default();
            let userinfo_endpoint: Option<String> =
                row.try_get("oidc_userinfo_endpoint").unwrap_or(None);
            let jwks_uri: String = row.try_get("oidc_jwks_uri").unwrap_or_default();
            let configured_at: i64 = row.try_get("oidc_configured_at").unwrap_or(0);
            OidcConfigRecord {
                issuer_url,
                client_id,
                has_client_secret: has_client_secret != 0,
                authorization_endpoint,
                token_endpoint,
                userinfo_endpoint,
                jwks_uri,
                configured_at,
            }
        })
    };

    // OIDC secret
    let oidc_secret = {
        let encrypted: Option<String> = row.try_get("oidc_encrypted_client_secret")?;
        encrypted.map(|encrypted_client_secret| {
            let stored_at: i64 = row.try_get("oidc_secret_stored_at").unwrap_or(0);
            OidcSecretRecord {
                encrypted_client_secret,
                stored_at,
            }
        })
    };

    // Owner
    let owner = {
        let email: Option<String> = row.try_get("owner_email")?;
        email.map(|email| {
            let oidc_subject: Option<String> = row.try_get("owner_oidc_subject").unwrap_or(None);
            let created_at: i64 = row.try_get("owner_created_at").unwrap_or(0);
            OwnerRecord {
                email,
                oidc_subject,
                created_at,
            }
        })
    };

    let created_at: i64 = row.try_get("created_at")?;
    let updated_at: i64 = row.try_get("updated_at")?;

    Ok(SetupStateFile {
        schema_version: u32::try_from(schema_version).context("schema_version out of u32 range")?,
        instance_id,
        setup_state,
        bootstrap_token,
        setup_session,
        oidc_config,
        oidc_secret,
        owner,
        created_at,
        updated_at,
    })
}
