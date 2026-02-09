use oore_runner::{RunnerConfig, detect_capabilities, run_runner_forever};
use sqlx::{Row, SqlitePool};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::token::{generate_token, hash_token};
use crate::util::now_unix;

const DEFAULT_EMBEDDED_RUNNER_NAME: &str = "local-embedded-runner";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerMode {
    Embedded,
    External,
    Hybrid,
}

impl RunnerMode {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "embedded" => Some(Self::Embedded),
            "external" => Some(Self::External),
            "hybrid" => Some(Self::Hybrid),
            _ => None,
        }
    }

    pub fn from_env() -> Self {
        match std::env::var("OORED_RUNNER_MODE") {
            Ok(raw) => Self::parse(&raw).unwrap_or_else(|| {
                warn!(
                    mode = %raw,
                    "invalid OORED_RUNNER_MODE; falling back to embedded"
                );
                Self::Embedded
            }),
            Err(_) => Self::Embedded,
        }
    }
}

pub async fn start_if_enabled(
    pool: SqlitePool,
    daemon_url: String,
) -> anyhow::Result<Option<tokio::task::JoinHandle<()>>> {
    let mode = RunnerMode::from_env();
    if mode == RunnerMode::External {
        info!("embedded runner disabled (OORED_RUNNER_MODE=external)");
        return Ok(None);
    }

    let runner_name = std::env::var("OORED_EMBEDDED_RUNNER_NAME")
        .unwrap_or_else(|_| DEFAULT_EMBEDDED_RUNNER_NAME.to_string());

    let capabilities = detect_capabilities().await;
    let mut cfg = upsert_embedded_runner(&pool, &runner_name, &capabilities).await?;
    cfg.daemon_url = daemon_url.clone();

    info!(
        mode = ?mode,
        runner_id = %cfg.runner_id,
        runner_name = %cfg.name,
        "starting embedded local runner"
    );

    let handle = tokio::spawn(async move {
        if let Err(e) = run_runner_forever(cfg, Some(daemon_url)).await {
            error!(error = %e, "embedded runner terminated unexpectedly");
        }
    });

    Ok(Some(handle))
}

async fn upsert_embedded_runner(
    pool: &SqlitePool,
    name: &str,
    capabilities: &serde_json::Value,
) -> anyhow::Result<RunnerConfig> {
    let now = now_unix();
    let token = generate_token();
    let token_hash = hash_token(&token);
    let caps_json = capabilities.to_string();

    let existing =
        sqlx::query("SELECT id FROM runners WHERE name = ?1 AND registered_by IS NULL LIMIT 1")
            .bind(name)
            .fetch_optional(pool)
            .await?;

    let runner_id = if let Some(row) = existing {
        let id: String = row.get("id");

        sqlx::query(
            "UPDATE runners \
             SET token_hash = ?1, status = 'offline', capabilities = ?2, last_heartbeat_at = NULL, updated_at = ?3 \
             WHERE id = ?4",
        )
        .bind(&token_hash)
        .bind(&caps_json)
        .bind(now)
        .bind(&id)
        .execute(pool)
        .await?;

        id
    } else {
        let id = Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO runners (id, name, token_hash, status, capabilities, registered_by, created_at, updated_at) \
             VALUES (?1, ?2, ?3, 'offline', ?4, ?5, ?6, ?6)",
        )
        .bind(&id)
        .bind(name)
        .bind(&token_hash)
        .bind(&caps_json)
        .bind(Option::<String>::None)
        .bind(now)
        .execute(pool)
        .await?;

        id
    };

    Ok(RunnerConfig {
        runner_id,
        runner_token: token,
        daemon_url: String::new(),
        name: name.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::SetupStore;

    #[test]
    fn parse_runner_mode() {
        assert_eq!(RunnerMode::parse("embedded"), Some(RunnerMode::Embedded));
        assert_eq!(RunnerMode::parse("external"), Some(RunnerMode::External));
        assert_eq!(RunnerMode::parse("hybrid"), Some(RunnerMode::Hybrid));
        assert_eq!(RunnerMode::parse(" EMBEDDED "), Some(RunnerMode::Embedded));
        assert_eq!(RunnerMode::parse("invalid"), None);
    }

    #[tokio::test]
    async fn upsert_embedded_runner_rotates_token_on_restart() {
        let temp = tempfile::TempDir::new().expect("tempdir");
        let db_path = temp.path().join("embedded_runner.db");

        let store = SetupStore::connect(db_path)
            .await
            .expect("connect test store");
        store.init_if_missing().await.expect("init state");
        let pool = store.pool().clone();

        let caps = serde_json::json!({ "os": "macos", "arch": "arm64" });

        let first = upsert_embedded_runner(&pool, "test-embedded-runner", &caps)
            .await
            .expect("first upsert");
        let first_hash: String = sqlx::query_scalar("SELECT token_hash FROM runners WHERE id = ?1")
            .bind(&first.runner_id)
            .fetch_one(&pool)
            .await
            .expect("first hash");

        let second = upsert_embedded_runner(&pool, "test-embedded-runner", &caps)
            .await
            .expect("second upsert");
        let second_hash: String =
            sqlx::query_scalar("SELECT token_hash FROM runners WHERE id = ?1")
                .bind(&second.runner_id)
                .fetch_one(&pool)
                .await
                .expect("second hash");

        assert_eq!(first.runner_id, second.runner_id);
        assert_ne!(first.runner_token, second.runner_token);
        assert_ne!(first_hash, second_hash);

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM runners WHERE name = ?1 AND registered_by IS NULL",
        )
        .bind("test-embedded-runner")
        .fetch_one(&pool)
        .await
        .expect("runner count");
        assert_eq!(count, 1);
    }
}
