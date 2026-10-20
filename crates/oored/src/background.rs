//! Background monitoring tasks for lease timeouts, build timeouts, runner health,
//! and retention cleanup.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use oore_contract::{BuildStatus, RetentionCleanupTarget, RetentionPolicy};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::retention::{load_effective_policy, load_global_policy};
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
const SQLITE_BIND_LIMIT_HEADROOM: usize = 900;
const MONITOR_BATCH_SIZE: i64 = 100;
const RETENTION_BATCH_SIZE: usize = 100;
const RETENTION_RUN_BUDGET_SECS: u64 = 30;
const RETENTION_BACKLOG_RETRY_SECS: i64 = 60;
const TERMINAL_BUILD_STATUSES: [&str; 4] = ["succeeded", "failed", "canceled", "timed_out"];

struct CleanupArtifact {
    file_path: String,
    file_size: Option<i64>,
}

#[derive(Default)]
struct CleanupTotals {
    builds_expired: i64,
    artifacts_deleted: i64,
    bytes_reclaimed: i64,
}

struct CleanupBatch {
    candidates: usize,
    cleaned: usize,
}

/// Start all background monitoring tasks.
pub fn start_background_tasks(
    pool: SqlitePool,
    scheduler: Arc<Scheduler>,
    storage: Arc<RwLock<StorageBackend>>,
) {
    tokio::spawn(lease_timeout_monitor(pool.clone()));
    tokio::spawn(build_timeout_monitor(pool.clone(), scheduler.clone()));
    tokio::spawn(runner_heartbeat_monitor(pool.clone(), scheduler));
    tokio::spawn(retention_cleanup_monitor(pool.clone(), storage.clone()));
    tokio::spawn(stale_pending_artifact_monitor(
        pool.clone(),
        storage.clone(),
    ));
    tokio::spawn(expired_artifact_monitor(pool, storage));
}

/// Start the release rollout monitor. It needs the store client, so it takes
/// the whole app state rather than the pool.
pub fn start_release_rollout_monitor(state: Arc<crate::AppState>) {
    tokio::spawn(release_rollout_monitor(state));
}

/// Advance staged Google Play rollouts on schedule and halt on crash rate.
///
/// Runs every 60 seconds. See `releases::run_rollout_tick`.
async fn release_rollout_monitor(state: Arc<crate::AppState>) {
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        if let Err(error) = crate::releases::run_rollout_tick(
            crate::releases::ReleaseDeps::from(&*state),
            now_unix(),
            MONITOR_BATCH_SIZE,
        )
        .await
        {
            error!(error = %error, "release_rollout_monitor: tick failed");
        }
    }
}

async fn stale_pending_artifact_monitor(pool: SqlitePool, storage: Arc<RwLock<StorageBackend>>) {
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        let cutoff = now_unix() - 30 * 60;
        let rows = match sqlx::query(
            "SELECT id, file_path FROM artifacts \
             WHERE state = 'pending' AND created_at < ?1 \
             ORDER BY created_at LIMIT ?2",
        )
        .bind(cutoff)
        .bind(MONITOR_BATCH_SIZE)
        .fetch_all(&pool)
        .await
        {
            Ok(rows) => rows,
            Err(error) => {
                warn!(error = %error, "stale_pending_artifact_monitor: query failed");
                continue;
            }
        };
        let backend = storage.read().await.clone();
        for row in rows {
            let artifact_id: String = row.get("id");
            let file_path: String = row.get("file_path");
            if let Err(error) = backend.delete_object(&file_path).await {
                warn!(artifact_id = %artifact_id, error = %error, "stale_pending_artifact_monitor: storage cleanup failed");
                continue;
            }
            if let Err(error) = sqlx::query(
                "UPDATE artifacts SET state = 'failed', finalized_at = ?1, error_message = 'upload reservation expired' WHERE id = ?2 AND state = 'pending'",
            )
            .bind(now_unix())
            .bind(&artifact_id)
            .execute(&pool)
            .await
            {
                warn!(artifact_id = %artifact_id, error = %error, "stale_pending_artifact_monitor: state update failed");
            }
        }
    }
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
            "SELECT id FROM builds WHERE status = 'assigned' AND updated_at < ?1 \
             ORDER BY updated_at LIMIT ?2",
        )
        .bind(cutoff)
        .bind(MONITOR_BATCH_SIZE)
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

        let rows = match sqlx::query(
            "SELECT id FROM builds WHERE status = 'running' AND started_at < ?1 \
                 ORDER BY started_at LIMIT ?2",
        )
        .bind(cutoff)
        .bind(MONITOR_BATCH_SIZE)
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
            "SELECT id, name, status FROM runners \
             WHERE status IN ('online', 'busy', 'draining') AND last_heartbeat_at < ?1 \
             ORDER BY last_heartbeat_at LIMIT ?2",
        )
        .bind(cutoff)
        .bind(MONITOR_BATCH_SIZE)
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
    let storage = storage.read().await.clone();
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

    let mut totals = CleanupTotals::default();

    let started_at = Instant::now();
    let budget = Duration::from_secs(RETENTION_RUN_BUDGET_SECS);
    let retry_soon;

    loop {
        let mut full_batch_seen = false;
        let mut made_progress = false;
        let mut failed_cleanup = false;
        let mut budget_exhausted = false;

        for project_row in &project_rows {
            let project_id: String = project_row.get("id");
            let batch = cleanup_project(pool, &storage, &project_id, now, &mut totals).await?;
            full_batch_seen |= batch.candidates == RETENTION_BATCH_SIZE;
            made_progress |= batch.cleaned > 0;
            failed_cleanup |= batch.cleaned < batch.candidates;

            if started_at.elapsed() >= budget {
                budget_exhausted = true;
                break;
            }
        }

        if policy.dry_run {
            retry_soon = false;
            break;
        }
        if budget_exhausted || (full_batch_seen && !made_progress) {
            retry_soon = true;
            break;
        }
        if !full_batch_seen {
            retry_soon = failed_cleanup;
            break;
        }
        tokio::task::yield_now().await;
    }

    if totals.builds_expired > 0 || policy.dry_run {
        let summary = serde_json::json!({
            "builds_expired": totals.builds_expired,
            "artifacts_deleted": totals.artifacts_deleted,
            "bytes_reclaimed": totals.bytes_reclaimed,
            "dry_run": policy.dry_run,
            "ran_at": now,
        });

        info!(
            builds_expired = totals.builds_expired,
            artifacts_deleted = totals.artifacts_deleted,
            bytes_reclaimed = totals.bytes_reclaimed,
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

    if retry_soon {
        Ok(policy
            .cleanup_interval_secs
            .min(RETENTION_BACKLOG_RETRY_SECS))
    } else {
        Ok(policy.cleanup_interval_secs)
    }
}

async fn cleanup_project(
    pool: &SqlitePool,
    storage: &StorageBackend,
    project_id: &str,
    now: i64,
    totals: &mut CleanupTotals,
) -> Result<CleanupBatch, anyhow::Error> {
    let policy = load_effective_policy(pool, project_id).await?;
    if !policy.enabled {
        return Ok(CleanupBatch {
            candidates: 0,
            cleaned: 0,
        });
    }

    let candidate_ids = collect_retention_candidates(pool, project_id, &policy, now).await?;
    let candidate_count = candidate_ids.len();
    let artifacts_by_build = load_artifacts_for_builds(pool, &candidate_ids).await?;
    let mut cleaned = 0;

    for build_id in &candidate_ids {
        let artifacts = artifacts_by_build
            .get(build_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);

        if policy.dry_run {
            totals.artifacts_deleted += artifacts.len() as i64;
            totals.bytes_reclaimed += artifacts
                .iter()
                .map(|artifact| artifact.file_size.unwrap_or(0))
                .sum::<i64>();
            totals.builds_expired += 1;
            info!(%build_id, %project_id, "retention_cleanup: dry run candidate");
            continue;
        }

        if !delete_artifact_files(storage, build_id, artifacts, totals).await {
            continue;
        }
        if cleanup_build_record(pool, build_id, policy.cleanup_target).await {
            totals.builds_expired += 1;
            cleaned += 1;
        }
    }

    Ok(CleanupBatch {
        candidates: candidate_count,
        cleaned,
    })
}

async fn collect_retention_candidates(
    pool: &SqlitePool,
    project_id: &str,
    policy: &RetentionPolicy,
    now: i64,
) -> Result<HashSet<String>, sqlx::Error> {
    let protected: HashSet<&str> = policy.keep_statuses.iter().map(String::as_str).collect();
    let eligible_statuses: Vec<&str> = TERMINAL_BUILD_STATUSES
        .iter()
        .copied()
        .filter(|status| !protected.contains(status))
        .collect();
    if eligible_statuses.is_empty() {
        return Ok(HashSet::new());
    }

    let mut candidate_ids = HashSet::new();
    if let Some(max_age_days) = policy.max_age_days {
        candidate_ids.extend(
            load_expired_build_ids(
                pool,
                project_id,
                &eligible_statuses,
                now - max_age_days * 86400,
                RETENTION_BATCH_SIZE,
            )
            .await?,
        );
    }

    if let Some(max_count) = policy.max_builds_per_project
        && candidate_ids.len() < RETENTION_BATCH_SIZE
    {
        candidate_ids.extend(
            load_excess_build_ids(
                pool,
                project_id,
                &eligible_statuses,
                max_count,
                RETENTION_BATCH_SIZE - candidate_ids.len(),
            )
            .await?,
        );
    }

    if let Some(max_size) = policy.max_artifact_size_bytes
        && candidate_ids.len() < RETENTION_BATCH_SIZE
    {
        add_size_limit_candidates(
            pool,
            project_id,
            &eligible_statuses,
            max_size,
            &mut candidate_ids,
        )
        .await?;
    }

    Ok(candidate_ids)
}

async fn add_size_limit_candidates(
    pool: &SqlitePool,
    project_id: &str,
    statuses: &[&str],
    max_size: i64,
    candidate_ids: &mut HashSet<String>,
) -> Result<(), sqlx::Error> {
    let total_size: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(a.file_size), 0) \
         FROM artifacts a JOIN builds b ON a.build_id = b.id \
         WHERE b.project_id = ?1",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    if total_size <= max_size {
        return Ok(());
    }

    let candidates = load_oldest_build_sizes(
        pool,
        project_id,
        statuses,
        RETENTION_BATCH_SIZE - candidate_ids.len(),
    )
    .await?;
    let mut remaining = total_size;
    for (id, build_size) in candidates {
        if remaining <= max_size {
            break;
        }
        candidate_ids.insert(id);
        remaining -= build_size;
    }
    Ok(())
}

async fn delete_artifact_files(
    storage: &StorageBackend,
    build_id: &str,
    artifacts: &[CleanupArtifact],
    totals: &mut CleanupTotals,
) -> bool {
    for artifact in artifacts {
        if let Err(error) = storage.delete_object(&artifact.file_path).await {
            warn!(%build_id, file_path = %artifact.file_path, %error, "retention_cleanup: storage deletion failed");
            return false;
        }
        totals.bytes_reclaimed += artifact.file_size.unwrap_or(0);
        totals.artifacts_deleted += 1;
    }
    true
}

async fn cleanup_build_record(
    pool: &SqlitePool,
    build_id: &str,
    target: RetentionCleanupTarget,
) -> bool {
    let result = match target {
        RetentionCleanupTarget::ArtifactsOnly => {
            if let Err(error) = sqlx::query("DELETE FROM artifacts WHERE build_id = ?1")
                .bind(build_id)
                .execute(pool)
                .await
            {
                Err(anyhow::Error::from(error))
            } else {
                crate::builds::transition_build(
                    pool,
                    build_id,
                    BuildStatus::Expired,
                    None,
                    Some("retention policy cleanup"),
                )
                .await
                .map(|_| ())
                .map_err(|error| anyhow::anyhow!("{error:?}"))
            }
        }
        RetentionCleanupTarget::Full => sqlx::query("DELETE FROM builds WHERE id = ?1")
            .bind(build_id)
            .execute(pool)
            .await
            .map(|_| ())
            .map_err(anyhow::Error::from),
    };

    match result {
        Ok(()) => {
            info!(%build_id, cleanup_target = %target, "retention_cleanup: build cleaned");
            true
        }
        Err(error) => {
            warn!(%build_id, %error, "retention_cleanup: database cleanup failed");
            false
        }
    }
}

fn push_status_filter(query: &mut QueryBuilder<Sqlite>, statuses: &[&str]) {
    query.push(" AND status IN (");
    let mut separated = query.separated(", ");
    for status in statuses {
        separated.push_bind(*status);
    }
    separated.push_unseparated(")");
}

async fn load_expired_build_ids(
    pool: &SqlitePool,
    project_id: &str,
    statuses: &[&str],
    age_cutoff: i64,
    limit: usize,
) -> Result<Vec<String>, sqlx::Error> {
    let mut query = QueryBuilder::<Sqlite>::new("SELECT id FROM builds WHERE project_id = ");
    query.push_bind(project_id);
    push_status_filter(&mut query, statuses);
    query
        .push(" AND finished_at IS NOT NULL AND finished_at < ")
        .push_bind(age_cutoff)
        .push(" ORDER BY finished_at LIMIT ")
        .push_bind(limit as i64);
    query.build_query_scalar().fetch_all(pool).await
}

async fn load_excess_build_ids(
    pool: &SqlitePool,
    project_id: &str,
    statuses: &[&str],
    keep_count: i64,
    limit: usize,
) -> Result<Vec<String>, sqlx::Error> {
    let mut query = QueryBuilder::<Sqlite>::new("SELECT id FROM builds WHERE project_id = ");
    query.push_bind(project_id);
    push_status_filter(&mut query, statuses);
    query
        .push(" ORDER BY finished_at DESC LIMIT ")
        .push_bind(limit as i64)
        .push(" OFFSET ")
        .push_bind(keep_count);
    query.build_query_scalar().fetch_all(pool).await
}

async fn load_oldest_build_sizes(
    pool: &SqlitePool,
    project_id: &str,
    statuses: &[&str],
    limit: usize,
) -> Result<Vec<(String, i64)>, sqlx::Error> {
    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT b.id, COALESCE(SUM(a.file_size), 0) AS build_artifact_size \
         FROM builds b LEFT JOIN artifacts a ON a.build_id = b.id \
         WHERE b.project_id = ",
    );
    query.push_bind(project_id).push(" AND b.status IN (");
    let mut separated = query.separated(", ");
    for status in statuses {
        separated.push_bind(*status);
    }
    separated.push_unseparated(")");
    query
        .push(" GROUP BY b.id ORDER BY b.finished_at ASC LIMIT ")
        .push_bind(limit as i64);

    query.build().fetch_all(pool).await.map(|rows| {
        rows.into_iter()
            .map(|row| (row.get("id"), row.get("build_artifact_size")))
            .collect()
    })
}

async fn load_artifacts_for_builds(
    pool: &SqlitePool,
    build_ids: &HashSet<String>,
) -> Result<HashMap<String, Vec<CleanupArtifact>>, sqlx::Error> {
    let mut artifacts_by_build: HashMap<String, Vec<CleanupArtifact>> = HashMap::new();
    if build_ids.is_empty() {
        return Ok(artifacts_by_build);
    }

    let build_ids: Vec<&str> = build_ids.iter().map(String::as_str).collect();
    for chunk in build_ids.chunks(SQLITE_BIND_LIMIT_HEADROOM) {
        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT build_id, file_path, file_size FROM artifacts WHERE build_id IN (",
        );
        let mut separated = query.separated(", ");
        for build_id in chunk {
            separated.push_bind(*build_id);
        }
        separated.push_unseparated(")");

        let rows = query.build().fetch_all(pool).await?;
        for row in rows {
            let build_id: String = row.get("build_id");
            artifacts_by_build
                .entry(build_id)
                .or_default()
                .push(CleanupArtifact {
                    file_path: row.get("file_path"),
                    file_size: row.get("file_size"),
                });
        }
    }

    Ok(artifacts_by_build)
}

/// Expired artifact and download token cleanup monitor.
///
/// Runs every 5 minutes. Deletes artifacts whose `expires_at` has passed
/// (files from storage first, then DB rows). Also prunes expired/revoked
/// download tokens.
async fn expired_artifact_monitor(pool: SqlitePool, storage: Arc<RwLock<StorageBackend>>) {
    // Wait 60 seconds on startup before first check
    tokio::time::sleep(Duration::from_secs(60)).await;

    loop {
        let now = now_unix();

        // 1. Clean up expired artifacts
        let expired_artifacts = match sqlx::query(
            "SELECT a.id, a.file_path, a.file_size, a.build_id \
             FROM artifacts a \
             WHERE a.expires_at IS NOT NULL AND a.expires_at < ?1 \
             ORDER BY a.expires_at LIMIT ?2",
        )
        .bind(now)
        .bind(MONITOR_BATCH_SIZE)
        .fetch_all(&pool)
        .await
        {
            Ok(rows) => rows,
            Err(e) => {
                error!(error = %e, "expired_artifact_monitor: failed to query expired artifacts");
                tokio::time::sleep(Duration::from_secs(300)).await;
                continue;
            }
        };

        let mut artifacts_deleted: i64 = 0;
        let mut bytes_reclaimed: i64 = 0;
        let backend = storage.read().await.clone();

        for row in &expired_artifacts {
            let artifact_id: String = row.get("id");
            let file_path: String = row.get("file_path");
            let file_size: Option<i64> = row.get("file_size");

            // Delete from storage first
            if let Err(e) = backend.delete_object(&file_path).await {
                warn!(
                    artifact_id = %artifact_id,
                    file_path = %file_path,
                    error = %e,
                    "expired_artifact_monitor: failed to delete artifact file, skipping"
                );
                continue;
            }
            // Delete DB row (CASCADE deletes download tokens too)
            if let Err(e) = sqlx::query("DELETE FROM artifacts WHERE id = ?1")
                .bind(&artifact_id)
                .execute(&pool)
                .await
            {
                warn!(artifact_id = %artifact_id, error = %e, "expired_artifact_monitor: failed to delete artifact row");
            } else {
                artifacts_deleted += 1;
                bytes_reclaimed += file_size.unwrap_or(0);
            }
        }

        // 2. Clean up expired/revoked download tokens (belt-and-suspenders alongside CASCADE)
        let tokens_deleted = match sqlx::query(
            "DELETE FROM artifact_download_tokens \
             WHERE id IN ( \
                 SELECT id FROM artifact_download_tokens \
                 WHERE expires_at < ?1 OR revoked_at IS NOT NULL \
                 ORDER BY expires_at LIMIT ?2 \
             )",
        )
        .bind(now)
        .bind(MONITOR_BATCH_SIZE)
        .execute(&pool)
        .await
        {
            Ok(result) => result.rows_affected() as i64,
            Err(e) => {
                warn!(error = %e, "expired_artifact_monitor: failed to clean up expired download tokens");
                0
            }
        };

        if artifacts_deleted > 0 || tokens_deleted > 0 {
            info!(
                artifacts_deleted,
                bytes_reclaimed, tokens_deleted, "expired_artifact_monitor: cleanup completed"
            );

            let summary = serde_json::json!({
                "artifacts_deleted": artifacts_deleted,
                "bytes_reclaimed": bytes_reclaimed,
                "tokens_deleted": tokens_deleted,
                "ran_at": now,
            });

            let _ = write_audit_log(
                &pool,
                None,
                "artifact_expiry_cleanup_completed",
                "artifact",
                None,
                Some(&summary.to_string()),
            )
            .await;
        }

        tokio::time::sleep(Duration::from_secs(300)).await;
    }
}
