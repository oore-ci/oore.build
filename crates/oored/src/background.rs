//! Background monitoring tasks for lease timeouts, build timeouts, and runner health.

use std::sync::Arc;
use std::time::Duration;

use oore_contract::BuildStatus;
use sqlx::{Row, SqlitePool};
use tracing::{error, info, warn};

use crate::scheduler::{BuildStateEvent, Scheduler};
use crate::util::now_unix;

/// Default lease timeout for assigned builds (5 minutes).
const LEASE_TIMEOUT_SECS: i64 = 300;

/// Default build timeout (60 minutes).
const BUILD_TIMEOUT_SECS: i64 = 3600;

/// Runner heartbeat staleness threshold (2 minutes).
const HEARTBEAT_STALE_SECS: i64 = 120;

/// Start all background monitoring tasks.
pub fn start_background_tasks(pool: SqlitePool, scheduler: Arc<Scheduler>) {
    tokio::spawn(lease_timeout_monitor(pool.clone()));
    tokio::spawn(build_timeout_monitor(pool.clone(), scheduler));
    tokio::spawn(runner_heartbeat_monitor(pool));
}

/// Monitor assigned builds whose lease has expired.
///
/// Runs every 30 seconds. Finds builds with status 'assigned' whose updated_at
/// is older than LEASE_TIMEOUT_SECS, transitions them back to 'queued', and
/// clears the stale runner_id. Runners claim directly from SQLite so no
/// in-memory re-enqueue is needed.
async fn lease_timeout_monitor(pool: SqlitePool) {
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;

        let now = now_unix();
        let cutoff = now - LEASE_TIMEOUT_SECS;

        let rows = match sqlx::query(
            "SELECT id FROM builds WHERE status = 'assigned' AND updated_at < ?1",
        )
        .bind(cutoff)
        .fetch_all(&pool)
        .await
        {
            Ok(rows) => rows,
            Err(e) => {
                error!(error = %e, "lease_timeout_monitor: failed to query assigned builds");
                continue;
            }
        };

        for row in rows {
            let build_id: String = row.get("id");

            match crate::builds::transition_build(
                &pool,
                &build_id,
                BuildStatus::Queued,
                None,
                Some("lease timeout"),
            )
            .await
            {
                Ok(_build) => {
                    // Clear stale runner_id so the old runner can no longer mutate this build
                    if let Err(e) = sqlx::query("UPDATE builds SET runner_id = NULL WHERE id = ?1")
                        .bind(&build_id)
                        .execute(&pool)
                        .await
                    {
                        warn!(build_id = %build_id, error = %e, "lease_timeout_monitor: failed to clear runner_id");
                    }

                    info!(build_id = %build_id, "lease_timeout_monitor: requeued build after lease timeout");
                }
                Err(e) => {
                    warn!(build_id = %build_id, error = ?e, "lease_timeout_monitor: failed to transition build to queued");
                }
            }
        }
    }
}

/// Monitor running builds that have exceeded the maximum build timeout.
///
/// Runs every 60 seconds. Finds builds with status 'running' whose started_at
/// is older than BUILD_TIMEOUT_SECS and transitions them to 'timed_out'.
async fn build_timeout_monitor(pool: SqlitePool, scheduler: Arc<Scheduler>) {
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;

        let now = now_unix();
        let cutoff = now - BUILD_TIMEOUT_SECS;

        let rows =
            match sqlx::query("SELECT id FROM builds WHERE status = 'running' AND started_at < ?1")
                .bind(cutoff)
                .fetch_all(&pool)
                .await
            {
                Ok(rows) => rows,
                Err(e) => {
                    error!(error = %e, "build_timeout_monitor: failed to query running builds");
                    continue;
                }
            };

        for row in rows {
            let build_id: String = row.get("id");

            match crate::builds::transition_build(
                &pool,
                &build_id,
                BuildStatus::TimedOut,
                None,
                Some("build timeout exceeded"),
            )
            .await
            {
                Ok(_build) => {
                    info!(build_id = %build_id, "build_timeout_monitor: timed out build");

                    scheduler.publish_event(BuildStateEvent {
                        build_id: build_id.clone(),
                        from_status: Some("running".to_string()),
                        to_status: "timed_out".to_string(),
                        actor: None,
                        reason: Some("build timeout exceeded".to_string()),
                        timestamp: now,
                    });
                }
                Err(e) => {
                    warn!(build_id = %build_id, error = ?e, "build_timeout_monitor: failed to transition build to timed_out");
                }
            }
        }
    }
}

/// Monitor runner heartbeats and mark stale runners as offline.
///
/// Runs every 60 seconds. Finds runners with status 'online', 'busy', or
/// 'draining' whose last_heartbeat_at is older than HEARTBEAT_STALE_SECS
/// and updates their status to 'offline'.
async fn runner_heartbeat_monitor(pool: SqlitePool) {
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;

        let now = now_unix();
        let cutoff = now - HEARTBEAT_STALE_SECS;

        let rows = match sqlx::query(
            "SELECT id, name, status FROM runners WHERE status IN ('online', 'busy', 'draining') AND last_heartbeat_at < ?1",
        )
        .bind(cutoff)
        .fetch_all(&pool)
        .await
        {
            Ok(rows) => rows,
            Err(e) => {
                error!(error = %e, "runner_heartbeat_monitor: failed to query stale runners");
                continue;
            }
        };

        for row in rows {
            let runner_id: String = row.get("id");
            let runner_name: String = row.get("name");
            let prev_status: String = row.get("status");

            match sqlx::query(
                "UPDATE runners SET status = 'offline', updated_at = ?1 WHERE id = ?2 AND status IN ('online', 'busy', 'draining')",
            )
            .bind(now)
            .bind(&runner_id)
            .execute(&pool)
            .await
            {
                Ok(result) => {
                    if result.rows_affected() > 0 {
                        info!(
                            runner_id = %runner_id,
                            runner_name = %runner_name,
                            prev_status = %prev_status,
                            "runner_heartbeat_monitor: marked runner as offline (stale heartbeat)"
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        runner_id = %runner_id,
                        error = %e,
                        "runner_heartbeat_monitor: failed to mark runner offline"
                    );
                }
            }
        }
    }
}
