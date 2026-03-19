//! Background monitoring tasks for lease timeouts, build timeouts, runner health,
//! and retention cleanup.

use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use oore_contract::{BuildStatus, RetentionCleanupTarget};
use sqlx::{Row, SqlitePool};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::retention::load_global_policy;
use crate::scheduler::{BuildStateEvent, RunnerStateEvent, Scheduler};
use crate::storage::StorageBackend;
use crate::store::write_audit_log;
use crate::util::now_unix;

/// Default lease timeout for assigned builds (5 minutes).
const LEASE_TIMEOUT_SECS: i64 = 300;

/// Default build timeout (60 minutes).
const BUILD_TIMEOUT_SECS: i64 = 3600;

/// Runner heartbeat staleness threshold (2 minutes).
const HEARTBEAT_STALE_SECS: i64 = 120;

/// Start all background monitoring tasks.
pub fn start_background_tasks(
    pool: SqlitePool,
    scheduler: Arc<Scheduler>,
    storage: Arc<RwLock<StorageBackend>>,
) {
    tokio::spawn(lease_timeout_monitor(pool.clone()));
    tokio::spawn(build_timeout_monitor(pool.clone(), scheduler.clone()));
    tokio::spawn(runner_heartbeat_monitor(pool.clone(), scheduler));
    tokio::spawn(retention_cleanup_monitor(pool, storage));
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
async fn runner_heartbeat_monitor(pool: SqlitePool, scheduler: Arc<Scheduler>) {
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

                        scheduler.publish_runner_event(RunnerStateEvent {
                            runner_id: runner_id.clone(),
                            runner_name: runner_name.clone(),
                            from_status: prev_status.clone(),
                            to_status: "offline".to_string(),
                            timestamp: now,
                        });
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

/// Retention cleanup monitor.
///
/// Runs at a configurable interval (default: 1 hour). Loads the global retention
/// policy and per-project overrides, finds candidate builds for cleanup, and
/// either expires artifacts or fully deletes builds depending on the cleanup target.
async fn retention_cleanup_monitor(pool: SqlitePool, storage: Arc<RwLock<StorageBackend>>) {
    // Wait 60 seconds on startup before first check
    tokio::time::sleep(Duration::from_secs(60)).await;

    loop {
        let interval_secs = match run_retention_cleanup(&pool, &storage).await {
            Ok(interval) => interval,
            Err(e) => {
                error!(error = %e, "retention_cleanup_monitor: cleanup run failed");
                3600 // fallback to 1 hour on error
            }
        };

        tokio::time::sleep(Duration::from_secs(interval_secs as u64)).await;
    }
}

/// Execute a single retention cleanup run. Returns the configured interval for the next run.
async fn run_retention_cleanup(
    pool: &SqlitePool,
    storage: &Arc<RwLock<StorageBackend>>,
) -> Result<i64, anyhow::Error> {
    let policy = load_global_policy(pool)
        .await
        .map_err(|e| anyhow::anyhow!("failed to load retention policy: {e}"))?;

    if !policy.enabled {
        return Ok(policy.cleanup_interval_secs);
    }

    let now = now_unix();

    // Load all project IDs
    let project_rows = sqlx::query("SELECT id FROM projects")
        .fetch_all(pool)
        .await?;

    let mut total_builds_expired: i64 = 0;
    let mut total_artifacts_deleted: i64 = 0;
    let mut total_bytes_reclaimed: i64 = 0;

    for project_row in &project_rows {
        let project_id: String = project_row.get("id");

        // Load project override if any, merge with global
        let effective = load_effective_project_policy(pool, &policy, &project_id).await?;

        if !effective.enabled {
            continue;
        }

        let effective_keep: HashSet<String> = effective.keep_statuses.iter().cloned().collect();
        let mut candidate_ids: HashSet<String> = HashSet::new();

        // Criterion 1: max age
        if let Some(max_age_days) = effective.max_age_days {
            let age_cutoff = now - max_age_days * 86400;
            let rows = sqlx::query(
                "SELECT id, status FROM builds \
                 WHERE project_id = ?1 \
                 AND status IN ('succeeded', 'failed', 'canceled', 'timed_out') \
                 AND finished_at IS NOT NULL AND finished_at < ?2",
            )
            .bind(&project_id)
            .bind(age_cutoff)
            .fetch_all(pool)
            .await?;

            for row in rows {
                let status: String = row.get("status");
                if !effective_keep.contains(&status) {
                    candidate_ids.insert(row.get("id"));
                }
            }
        }

        // Criterion 2: max count per project
        // Protected (keep_statuses) builds don't consume retention slots.
        if let Some(max_count) = effective.max_builds_per_project {
            let rows = sqlx::query(
                "SELECT id, status FROM builds \
                 WHERE project_id = ?1 \
                 AND status IN ('succeeded', 'failed', 'canceled', 'timed_out') \
                 ORDER BY finished_at DESC",
            )
            .bind(&project_id)
            .fetch_all(pool)
            .await?;

            // Filter out protected builds so they don't count toward the limit
            let non_protected: Vec<_> = rows
                .iter()
                .filter(|row| {
                    let status: String = row.get("status");
                    !effective_keep.contains(&status)
                })
                .collect();

            for row in non_protected.iter().skip(max_count as usize) {
                candidate_ids.insert(row.get("id"));
            }
        }

        // Criterion 3: max artifact size per project
        if let Some(max_size) = effective.max_artifact_size_bytes {
            let total_row = sqlx::query(
                "SELECT COALESCE(SUM(a.file_size), 0) as total_size \
                 FROM artifacts a JOIN builds b ON a.build_id = b.id \
                 WHERE b.project_id = ?1",
            )
            .bind(&project_id)
            .fetch_one(pool)
            .await?;

            let total_size: i64 = total_row.get("total_size");

            if total_size > max_size {
                let mut remaining = total_size;
                // Get builds ordered oldest first, with their artifact sizes
                let rows = sqlx::query(
                    "SELECT b.id, b.status, COALESCE(SUM(a.file_size), 0) as build_artifact_size \
                     FROM builds b LEFT JOIN artifacts a ON a.build_id = b.id \
                     WHERE b.project_id = ?1 \
                     AND b.status IN ('succeeded', 'failed', 'canceled', 'timed_out') \
                     GROUP BY b.id \
                     ORDER BY b.finished_at ASC",
                )
                .bind(&project_id)
                .fetch_all(pool)
                .await?;

                for row in rows {
                    if remaining <= max_size {
                        break;
                    }
                    let status: String = row.get("status");
                    let id: String = row.get("id");
                    let build_size: i64 = row.get("build_artifact_size");
                    if !effective_keep.contains(&status) {
                        candidate_ids.insert(id);
                        remaining -= build_size;
                    }
                }
            }
        }

        // Process candidates
        for build_id in &candidate_ids {
            // Load artifacts for this build (needed for both dry-run counting and real deletion)
            let artifacts =
                sqlx::query("SELECT id, file_path, file_size FROM artifacts WHERE build_id = ?1")
                    .bind(build_id)
                    .fetch_all(pool)
                    .await?;

            if policy.dry_run {
                // Count what *would* be cleaned up so dry-run can preview storage impact
                for artifact in &artifacts {
                    let file_size: Option<i64> = artifact.get("file_size");
                    total_bytes_reclaimed += file_size.unwrap_or(0);
                    total_artifacts_deleted += 1;
                }
                info!(build_id = %build_id, project_id = %project_id, "retention_cleanup: [DRY RUN] would clean up build");
                total_builds_expired += 1;
                continue;
            }

            // Delete artifact files from storage first, then delete DB rows only on success.
            // This prevents orphaned files when storage deletion fails.
            let mut all_files_deleted = true;
            for artifact in &artifacts {
                let file_path: String = artifact.get("file_path");
                let file_size: Option<i64> = artifact.get("file_size");

                let backend = storage.read().await;
                if let Err(e) = backend.delete_object(&file_path).await {
                    warn!(
                        build_id = %build_id,
                        file_path = %file_path,
                        error = %e,
                        "retention_cleanup: failed to delete artifact file, skipping DB cleanup for this build"
                    );
                    all_files_deleted = false;
                    break;
                } else {
                    total_bytes_reclaimed += file_size.unwrap_or(0);
                    total_artifacts_deleted += 1;
                }
            }

            if !all_files_deleted {
                // Skip DB deletion for this build to avoid orphaned storage files
                warn!(build_id = %build_id, "retention_cleanup: skipping build cleanup due to storage deletion failure");
                continue;
            }

            match effective.cleanup_target {
                RetentionCleanupTarget::ArtifactsOnly => {
                    // Delete artifact rows (storage files already deleted above)
                    if let Err(e) = sqlx::query("DELETE FROM artifacts WHERE build_id = ?1")
                        .bind(build_id)
                        .execute(pool)
                        .await
                    {
                        warn!(build_id = %build_id, error = %e, "retention_cleanup: failed to delete artifact rows");
                    }

                    // Transition build to Expired
                    match crate::builds::transition_build(
                        pool,
                        build_id,
                        BuildStatus::Expired,
                        None,
                        Some("retention policy cleanup"),
                    )
                    .await
                    {
                        Ok(_) => {
                            info!(build_id = %build_id, "retention_cleanup: expired build (artifacts_only)");
                        }
                        Err(e) => {
                            warn!(build_id = %build_id, error = ?e, "retention_cleanup: failed to transition build to expired");
                        }
                    }
                }
                RetentionCleanupTarget::Full => {
                    // Full delete — cascades to build_events, build_logs, artifacts
                    if let Err(e) = sqlx::query("DELETE FROM builds WHERE id = ?1")
                        .bind(build_id)
                        .execute(pool)
                        .await
                    {
                        warn!(build_id = %build_id, error = %e, "retention_cleanup: failed to delete build");
                    } else {
                        info!(build_id = %build_id, "retention_cleanup: fully deleted build");
                    }
                }
            }

            total_builds_expired += 1;
        }
    }

    if total_builds_expired > 0 || policy.dry_run {
        let summary = serde_json::json!({
            "builds_expired": total_builds_expired,
            "artifacts_deleted": total_artifacts_deleted,
            "bytes_reclaimed": total_bytes_reclaimed,
            "dry_run": policy.dry_run,
            "ran_at": now,
        });

        info!(
            builds_expired = total_builds_expired,
            artifacts_deleted = total_artifacts_deleted,
            bytes_reclaimed = total_bytes_reclaimed,
            dry_run = policy.dry_run,
            "retention_cleanup: run completed"
        );

        let _ = write_audit_log(
            pool,
            None,
            "retention_cleanup_completed",
            "retention_policy",
            Some("global"),
            Some(&summary.to_string()),
        )
        .await;
    }

    Ok(policy.cleanup_interval_secs)
}

/// Load the effective retention policy for a project (override merged with global).
async fn load_effective_project_policy(
    pool: &SqlitePool,
    global: &oore_contract::RetentionPolicy,
    project_id: &str,
) -> Result<oore_contract::RetentionPolicy, anyhow::Error> {
    let row = sqlx::query("SELECT * FROM project_retention_overrides WHERE project_id = ?1")
        .bind(project_id)
        .fetch_optional(pool)
        .await?;

    let Some(row) = row else {
        return Ok(global.clone());
    };

    Ok(oore_contract::RetentionPolicy {
        enabled: row
            .get::<Option<i32>, _>("enabled")
            .map(|v| v != 0)
            .unwrap_or(global.enabled),
        max_age_days: row
            .get::<Option<i64>, _>("max_age_days")
            .or(global.max_age_days),
        max_builds_per_project: row
            .get::<Option<i64>, _>("max_builds_per_project")
            .or(global.max_builds_per_project),
        max_artifact_size_bytes: row
            .get::<Option<i64>, _>("max_artifact_size_bytes")
            .or(global.max_artifact_size_bytes),
        cleanup_target: row
            .get::<Option<String>, _>("cleanup_target")
            .and_then(|s| RetentionCleanupTarget::from_str(&s).ok())
            .unwrap_or(global.cleanup_target),
        keep_statuses: row
            .get::<Option<String>, _>("keep_statuses")
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| global.keep_statuses.clone()),
        dry_run: global.dry_run,
        cleanup_interval_secs: global.cleanup_interval_secs,
        updated_at: Some(row.get::<i64, _>("updated_at")),
    })
}
