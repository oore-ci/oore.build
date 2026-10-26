//! Background monitoring tasks for lease timeouts, build timeouts, runner health,
//! and retention cleanup.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use oore_contract::{ApprovalPolicy, BuildStatus, RetentionCleanupTarget, RetentionPolicy};
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

/// Build timeout for builds whose snapshot predates per-pipeline timeouts.
const DEFAULT_BUILD_TIMEOUT_SECS: i64 = oore_contract::DEFAULT_BUILD_TIMEOUT_MINUTES as i64 * 60;
/// How often running builds are checked against their timeout.
const BUILD_TIMEOUT_CHECK_INTERVAL: Duration = Duration::from_secs(15);

/// Runner heartbeat staleness threshold (2 minutes).
const HEARTBEAT_STALE_SECS: i64 = 120;
/// How long a runner may miss heartbeats before its running build fails.
const RUNNER_LOST_SECS: i64 = 300;
/// A claim leaves a build in scheduled only while its transaction runs, so
/// one older than this was interrupted.
const SCHEDULED_TIMEOUT_SECS: i64 = 60;
const SQLITE_BIND_LIMIT_HEADROOM: usize = 900;
const MONITOR_BATCH_SIZE: i64 = 100;
const RETENTION_BATCH_SIZE: usize = 100;
const RETENTION_RUN_BUDGET_SECS: u64 = 30;
const RETENTION_BACKLOG_RETRY_SECS: i64 = 60;
const TERMINAL_BUILD_STATUSES: [&str; 4] = ["succeeded", "failed", "canceled", "timed_out"];

/// Which builds a cleanup run may touch.
struct Eligible<'a> {
    /// Build statuses that are candidates.
    statuses: Vec<&'a str>,
    /// Outcomes that are candidates. For a build retention already expired this
    /// is the status it had before, so `keep_statuses` still protects it.
    outcomes: Vec<&'a str>,
}

impl<'a> Eligible<'a> {
    fn new(policy: &'a RetentionPolicy) -> Self {
        let protected: HashSet<&str> = policy.keep_statuses.iter().map(String::as_str).collect();
        let outcomes: Vec<&str> = TERMINAL_BUILD_STATUSES
            .iter()
            .copied()
            .chain(["expired"])
            .filter(|status| !protected.contains(status))
            .collect();
        let mut statuses: Vec<&str> = TERMINAL_BUILD_STATUSES
            .iter()
            .copied()
            .filter(|status| outcomes.contains(status))
            .collect();
        // Expired builds have no artifacts left; only Full cleanup has work to
        // do on them.
        if policy.cleanup_target == RetentionCleanupTarget::Full {
            statuses.push("expired");
        }
        Self { statuses, outcomes }
    }

    fn is_empty(&self) -> bool {
        self.statuses.is_empty()
    }

    /// Filter builds aliased `b` to eligible ones. A release that is not
    /// `complete` or `failed` keeps its build and artifacts; any release keeps
    /// an `expired` build so Full cleanup never deletes the release record.
    fn push_filter(&self, query: &mut QueryBuilder<Sqlite>) {
        query.push(" AND b.status IN (");
        let mut separated = query.separated(", ");
        for status in &self.statuses {
            separated.push_bind(*status);
        }
        separated.push_unseparated(")");
        query.push(" AND COALESCE(b.expired_from, b.status) IN (");
        let mut separated = query.separated(", ");
        for outcome in &self.outcomes {
            separated.push_bind(*outcome);
        }
        separated.push_unseparated(")");
        query.push(
            " AND NOT EXISTS (SELECT 1 FROM releases r WHERE r.build_id = b.id \
             AND (r.state NOT IN ('complete', 'failed') OR b.status = 'expired'))",
        );
    }
}

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

/// How long after its upload URL expires a pending artifact is given up on.
/// An upload that started just before the URL expired can still be running.
const STALE_PENDING_GRACE_SECS: i64 = 15 * 60;

async fn stale_pending_artifact_monitor(pool: SqlitePool, storage: Arc<RwLock<StorageBackend>>) {
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        let backend = storage.read().await.clone();
        if let Err(error) = sweep_stale_pending_artifacts(&pool, &backend, now_unix()).await {
            warn!(error = %error, "stale_pending_artifact_monitor: sweep failed");
        }
    }
}

/// Fail pending artifacts whose upload can no longer finish, then delete
/// whatever they stored. The state changes first, so a runner completing the
/// upload at the same moment either wins (the artifact stays available) or
/// gets a conflict, never an available artifact with no file.
async fn sweep_stale_pending_artifacts(
    pool: &SqlitePool,
    storage: &StorageBackend,
    now: i64,
) -> Result<(), sqlx::Error> {
    let cutoff = now - crate::artifacts::UPLOAD_URL_TTL_SECS as i64 - STALE_PENDING_GRACE_SECS;
    let failed: Vec<(String, String)> = sqlx::query_as(
        "UPDATE artifacts SET state = 'failed', finalized_at = ?1, \
         error_message = 'upload reservation expired' \
         WHERE id IN (SELECT id FROM artifacts WHERE state = 'pending' AND created_at < ?2 \
             ORDER BY created_at LIMIT ?3) \
         AND state = 'pending' \
         RETURNING id, file_path",
    )
    .bind(now)
    .bind(cutoff)
    .bind(MONITOR_BATCH_SIZE)
    .fetch_all(pool)
    .await?;
    for (artifact_id, file_path) in failed {
        if let Err(error) = storage.delete_object(&file_path).await {
            warn!(%artifact_id, %file_path, %error, "stale_pending_artifact_monitor: storage cleanup failed");
        }
    }
    Ok(())
}

/// Monitor assigned builds whose lease has expired.
///
/// Runs every 30 seconds. See [`requeue_expired_claims`].
async fn lease_timeout_monitor(pool: SqlitePool) {
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        if let Err(e) = requeue_expired_claims(&pool, now_unix()).await {
            error!(error = %e, "lease_timeout_monitor: failed to query claimed builds");
        }
    }
}

/// Moves builds a runner claimed but never started back to 'queued', which
/// clears the stale runner_id: 'assigned' builds older than
/// LEASE_TIMEOUT_SECS and 'scheduled' builds older than
/// SCHEDULED_TIMEOUT_SECS. Runners claim directly from SQLite so no
/// in-memory re-enqueue is needed. Returns the IDs it requeued.
pub(crate) async fn requeue_expired_claims(
    pool: &SqlitePool,
    now: i64,
) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, status FROM builds \
         WHERE (status = 'assigned' AND updated_at < ?1) \
            OR (status = 'scheduled' AND updated_at < ?2) \
         ORDER BY updated_at LIMIT ?3",
    )
    .bind(now - LEASE_TIMEOUT_SECS)
    .bind(now - SCHEDULED_TIMEOUT_SECS)
    .bind(MONITOR_BATCH_SIZE)
    .fetch_all(pool)
    .await?;

    let mut requeued = Vec::new();
    for row in rows {
        let build_id: String = row.get("id");
        let status: String = row.get("status");
        let reason = if status == "scheduled" {
            "runner claim was interrupted"
        } else {
            "lease timeout"
        };

        match crate::builds::transition_build(
            pool,
            &build_id,
            BuildStatus::Queued,
            None,
            Some(reason),
        )
        .await
        {
            Ok(_build) => {
                info!(build_id = %build_id, from = %status, "lease_timeout_monitor: requeued build");
                requeued.push(build_id);
            }
            Err(e) => {
                warn!(build_id = %build_id, error = ?e, "lease_timeout_monitor: failed to transition build to queued");
            }
        }
    }
    Ok(requeued)
}

/// Monitor running builds that have exceeded their pipeline's timeout.
///
/// Runs every 15 seconds. Each build carries its timeout in
/// `config_snapshot.timeout_secs`, captured when it was created.
async fn build_timeout_monitor(pool: SqlitePool, scheduler: Arc<Scheduler>) {
    loop {
        tokio::time::sleep(BUILD_TIMEOUT_CHECK_INTERVAL).await;
        if let Err(e) = time_out_overdue_builds(&pool, &scheduler, now_unix()).await {
            error!(error = %e, "build_timeout_monitor: failed to query running builds");
        }
        if let Err(e) = expire_pending_approvals(&pool, &scheduler, now_unix()).await {
            error!(error = %e, "build_timeout_monitor: failed to query pending approvals");
        }
    }
}

/// Cancel builds whose approval request was never decided within the
/// pipeline's `approval.expires_after_hours`. Returns the IDs it canceled.
pub(crate) async fn expire_pending_approvals(
    pool: &SqlitePool,
    scheduler: &Scheduler,
    now: i64,
) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT b.id, COALESCE(json_extract(p.approval_policy, '$.expires_after_hours'), ?1) * 3600 AS ttl \
         FROM builds b LEFT JOIN pipelines p ON p.id = b.pipeline_id \
         WHERE b.status = 'queued' AND b.approval_state = 'pending' \
           AND b.approval_requested_at \
               + COALESCE(json_extract(p.approval_policy, '$.expires_after_hours'), ?1) * 3600 <= ?2 \
         ORDER BY b.approval_requested_at LIMIT ?3",
    )
    .bind(i64::from(ApprovalPolicy::DEFAULT_EXPIRES_AFTER_HOURS))
    .bind(now)
    .bind(MONITOR_BATCH_SIZE)
    .fetch_all(pool)
    .await?;

    let mut expired = Vec::new();
    for row in rows {
        let build_id: String = row.get("id");
        let ttl: i64 = row.get("ttl");
        let hours = ttl / 3600;
        let reason = if hours % 24 == 0 {
            format!("approval request expired after {} days", hours / 24)
        } else {
            format!("approval request expired after {hours} hours")
        };
        match crate::builds::transition_build(
            pool,
            &build_id,
            BuildStatus::Canceled,
            None,
            Some(&reason),
        )
        .await
        {
            Ok(_build) => {
                info!(build_id = %build_id, "build_timeout_monitor: approval request expired");
                scheduler.publish_event(BuildStateEvent {
                    build_id: build_id.clone(),
                    from_status: Some("queued".to_string()),
                    to_status: "canceled".to_string(),
                    actor: None,
                    reason: Some(reason),
                    timestamp: now,
                });
                expired.push(build_id);
            }
            Err(e) => {
                warn!(build_id = %build_id, error = ?e, "build_timeout_monitor: failed to expire approval request");
            }
        }
    }
    Ok(expired)
}

/// Mark running builds past their timeout as `timed_out`. Returns the IDs
/// of the builds it timed out.
pub(crate) async fn time_out_overdue_builds(
    pool: &SqlitePool,
    scheduler: &Scheduler,
    now: i64,
) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, COALESCE(json_extract(config_snapshot, '$.timeout_secs'), ?1) AS timeout_secs \
         FROM builds WHERE status = 'running' \
           AND started_at + COALESCE(json_extract(config_snapshot, '$.timeout_secs'), ?1) <= ?2 \
         ORDER BY started_at LIMIT ?3",
    )
    .bind(DEFAULT_BUILD_TIMEOUT_SECS)
    .bind(now)
    .bind(MONITOR_BATCH_SIZE)
    .fetch_all(pool)
    .await?;

    let mut timed_out = Vec::new();
    for row in rows {
        let build_id: String = row.get("id");
        let timeout_minutes = row.get::<i64, _>("timeout_secs") / 60;
        let reason = format!("build timeout of {timeout_minutes} minutes exceeded");

        match crate::builds::transition_build(
            pool,
            &build_id,
            BuildStatus::TimedOut,
            None,
            Some(&reason),
        )
        .await
        {
            Ok(_build) => {
                info!(build_id = %build_id, timeout_minutes, "build_timeout_monitor: timed out build");

                scheduler.publish_event(BuildStateEvent {
                    build_id: build_id.clone(),
                    from_status: Some("running".to_string()),
                    to_status: "timed_out".to_string(),
                    actor: None,
                    reason: Some(reason),
                    timestamp: now,
                });
                timed_out.push(build_id);
            }
            Err(e) => {
                warn!(build_id = %build_id, error = ?e, "build_timeout_monitor: failed to transition build to timed_out");
            }
        }
    }
    Ok(timed_out)
}

/// Monitor runner heartbeats and mark stale runners as offline.
///
/// Runs every 60 seconds. Finds runners with status 'online', 'busy', or
/// 'draining' whose last_heartbeat_at is older than HEARTBEAT_STALE_SECS
/// and updates their status to 'offline'.
async fn runner_heartbeat_monitor(pool: SqlitePool, scheduler: Arc<Scheduler>) {
    let started = Instant::now();
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;

        let now = now_unix();
        // Heartbeats stop arriving while this daemon is down, so only judge
        // runners lost once they have had a full window to reach it again.
        if started.elapsed() >= Duration::from_secs(RUNNER_LOST_SECS as u64)
            && let Err(e) = fail_builds_of_lost_runners(&pool, &scheduler, now).await
        {
            error!(error = %e, "runner_heartbeat_monitor: failed to release builds of lost runners");
        }
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

/// Ends the builds held by runners that have missed heartbeats for
/// RUNNER_LOST_SECS: running builds fail, assigned ones go back to the
/// queue. Returns the IDs of the builds it failed.
pub(crate) async fn fail_builds_of_lost_runners(
    pool: &SqlitePool,
    scheduler: &Scheduler,
    now: i64,
) -> Result<Vec<String>, sqlx::Error> {
    let runner_ids: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT r.id FROM runners r JOIN builds b ON b.runner_id = r.id \
         WHERE b.status IN ('assigned', 'running') \
           AND COALESCE(r.last_heartbeat_at, 0) < ?1 \
         LIMIT ?2",
    )
    .bind(now - RUNNER_LOST_SECS)
    .bind(MONITOR_BATCH_SIZE)
    .fetch_all(pool)
    .await?;

    let mut failed = Vec::new();
    for runner_id in runner_ids {
        failed.extend(
            crate::runners::release_runner_builds(
                pool,
                scheduler,
                &runner_id,
                "Lost contact with the runner: it stopped sending heartbeats",
            )
            .await?,
        );
    }
    Ok(failed)
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
        // A full cleanup deletes the build row and its feedback with it, so
        // collect the screenshots first.
        let screenshot_keys = match policy.cleanup_target {
            RetentionCleanupTarget::Full => {
                match crate::build_feedback::screenshot_keys_for_build(pool, build_id).await {
                    Ok(keys) => keys,
                    Err(error) => {
                        warn!(%build_id, %error, "retention_cleanup: failed to list feedback screenshots");
                        continue;
                    }
                }
            }
            RetentionCleanupTarget::ArtifactsOnly => Vec::new(),
        };
        if cleanup_build_record(pool, build_id, policy.cleanup_target).await {
            totals.builds_expired += 1;
            cleaned += 1;
            if !screenshot_keys.is_empty() && !build_exists(pool, build_id).await {
                crate::build_feedback::delete_screenshots(storage, &screenshot_keys).await;
            }
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
    let eligible = Eligible::new(policy);
    if eligible.is_empty() {
        return Ok(HashSet::new());
    }

    let mut candidate_ids = HashSet::new();
    if let Some(max_age_days) = policy.max_age_days {
        candidate_ids.extend(
            load_expired_build_ids(
                pool,
                project_id,
                &eligible,
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
                &eligible,
                max_count,
                RETENTION_BATCH_SIZE - candidate_ids.len(),
            )
            .await?,
        );
    }

    if let Some(max_size) = policy.max_artifact_size_bytes
        && candidate_ids.len() < RETENTION_BATCH_SIZE
    {
        add_size_limit_candidates(pool, project_id, &eligible, max_size, &mut candidate_ids)
            .await?;
    }

    Ok(candidate_ids)
}

/// Add the oldest eligible builds until the project's stored artifacts fit
/// under `max_size`. Only available artifacts count, since pending and failed
/// uploads hold nothing. When protected builds alone are over the limit no
/// amount of cleanup can reach it, so nothing is removed.
async fn add_size_limit_candidates(
    pool: &SqlitePool,
    project_id: &str,
    eligible: &Eligible<'_>,
    max_size: i64,
    candidate_ids: &mut HashSet<String>,
) -> Result<(), sqlx::Error> {
    let total_size: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(a.file_size), 0) \
         FROM artifacts a JOIN builds b ON a.build_id = b.id \
         WHERE b.project_id = ?1 AND a.state = 'available'",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    if total_size <= max_size {
        return Ok(());
    }

    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT COALESCE(SUM(a.file_size), 0) FROM artifacts a JOIN builds b ON a.build_id = b.id \
         WHERE a.state = 'available' AND b.project_id = ",
    );
    query.push_bind(project_id);
    eligible.push_filter(&mut query);
    let eligible_size: i64 = query.build_query_scalar().fetch_one(pool).await?;
    let protected_size = total_size - eligible_size;
    if protected_size > max_size {
        warn!(
            %project_id,
            protected_size,
            max_size,
            "retention_cleanup: builds kept by keep_statuses or releases exceed the size limit"
        );
        return Ok(());
    }

    // Builds another limit already picked free their bytes too.
    let mut remaining = total_size - available_size(pool, candidate_ids).await?;
    if remaining <= max_size {
        return Ok(());
    }
    let candidates = load_oldest_build_sizes(
        pool,
        project_id,
        eligible,
        RETENTION_BATCH_SIZE - candidate_ids.len(),
    )
    .await?;
    for (id, build_size) in candidates {
        if remaining <= max_size {
            break;
        }
        if candidate_ids.insert(id) {
            remaining -= build_size;
        }
    }
    Ok(())
}

async fn available_size(
    pool: &SqlitePool,
    build_ids: &HashSet<String>,
) -> Result<i64, sqlx::Error> {
    if build_ids.is_empty() {
        return Ok(0);
    }
    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT COALESCE(SUM(file_size), 0) FROM artifacts \
         WHERE state = 'available' AND build_id IN (",
    );
    let mut separated = query.separated(", ");
    for build_id in build_ids {
        separated.push_bind(build_id.as_str());
    }
    separated.push_unseparated(")");
    query.build_query_scalar().fetch_one(pool).await
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
        RetentionCleanupTarget::ArtifactsOnly => expire_build_artifacts(pool, build_id).await,
        // A build with a release keeps its row, so the release and its history
        // survive; only its artifacts go.
        RetentionCleanupTarget::Full => match sqlx::query(
            "DELETE FROM builds WHERE id = ?1 \
             AND NOT EXISTS (SELECT 1 FROM releases WHERE build_id = ?1)",
        )
        .bind(build_id)
        .execute(pool)
        .await
        {
            Ok(done) if done.rows_affected() == 0 => expire_build_artifacts(pool, build_id).await,
            Ok(_) => Ok(()),
            Err(error) => Err(anyhow::Error::from(error)),
        },
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

/// Whether the build row is still there. A full cleanup keeps the row of a
/// build with a release. On a read error it answers yes, so nothing the row
/// still points at is removed.
async fn build_exists(pool: &SqlitePool, build_id: &str) -> bool {
    sqlx::query_scalar::<_, i64>("SELECT 1 FROM builds WHERE id = ?1")
        .bind(build_id)
        .fetch_optional(pool)
        .await
        .map_or(true, |row| row.is_some())
}

/// Delete a build's artifact rows and mark it `expired`, remembering the
/// outcome it had so insights still count it.
async fn expire_build_artifacts(pool: &SqlitePool, build_id: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM artifacts WHERE build_id = ?1")
        .bind(build_id)
        .execute(pool)
        .await?;
    sqlx::query("UPDATE builds SET expired_from = status WHERE id = ?1 AND status <> 'expired'")
        .bind(build_id)
        .execute(pool)
        .await?;
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

async fn load_expired_build_ids(
    pool: &SqlitePool,
    project_id: &str,
    eligible: &Eligible<'_>,
    age_cutoff: i64,
    limit: usize,
) -> Result<Vec<String>, sqlx::Error> {
    let mut query = QueryBuilder::<Sqlite>::new("SELECT b.id FROM builds b WHERE b.project_id = ");
    query.push_bind(project_id);
    eligible.push_filter(&mut query);
    query
        .push(" AND b.finished_at IS NOT NULL AND b.finished_at < ")
        .push_bind(age_cutoff)
        .push(" ORDER BY b.finished_at LIMIT ")
        .push_bind(limit as i64);
    query.build_query_scalar().fetch_all(pool).await
}

async fn load_excess_build_ids(
    pool: &SqlitePool,
    project_id: &str,
    eligible: &Eligible<'_>,
    keep_count: i64,
    limit: usize,
) -> Result<Vec<String>, sqlx::Error> {
    let mut query = QueryBuilder::<Sqlite>::new("SELECT b.id FROM builds b WHERE b.project_id = ");
    query.push_bind(project_id);
    eligible.push_filter(&mut query);
    query
        .push(" ORDER BY b.finished_at DESC LIMIT ")
        .push_bind(limit as i64)
        .push(" OFFSET ")
        .push_bind(keep_count);
    query.build_query_scalar().fetch_all(pool).await
}

/// The oldest eligible builds that still have stored artifacts, with their size.
async fn load_oldest_build_sizes(
    pool: &SqlitePool,
    project_id: &str,
    eligible: &Eligible<'_>,
    limit: usize,
) -> Result<Vec<(String, i64)>, sqlx::Error> {
    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT b.id, SUM(a.file_size) AS build_artifact_size \
         FROM builds b JOIN artifacts a ON a.build_id = b.id AND a.state = 'available' \
         WHERE b.project_id = ",
    );
    query.push_bind(project_id);
    eligible.push_filter(&mut query);
    query
        .push(" GROUP BY b.id HAVING SUM(a.file_size) > 0 ORDER BY b.finished_at ASC LIMIT ")
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

/// Artifacts past their `expires_at`, except those of a build whose release is
/// still in progress.
async fn load_expired_artifacts(
    pool: &SqlitePool,
    now: i64,
) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
    sqlx::query(
        "SELECT a.id, a.file_path, a.file_size, a.build_id \
         FROM artifacts a \
         WHERE a.expires_at IS NOT NULL AND a.expires_at < ?1 \
         AND NOT EXISTS (SELECT 1 FROM releases r WHERE r.build_id = a.build_id \
             AND r.state NOT IN ('complete', 'failed')) \
         ORDER BY a.expires_at LIMIT ?2",
    )
    .bind(now)
    .bind(MONITOR_BATCH_SIZE)
    .fetch_all(pool)
    .await
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
        let expired_artifacts = match load_expired_artifacts(&pool, now).await {
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

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;
    const NOW: i64 = 100 * DAY;

    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        for statement in [
            "INSERT INTO users (id, email, oidc_subject, role, created_at, updated_at) \
             VALUES ('u1', 'dev@example.com', 'sub', 'developer', 0, 0)",
            "INSERT INTO projects (id, name, created_by, created_at, updated_at) VALUES ('p1', 'App', 'u1', 0, 0)",
            "INSERT INTO pipelines (id, project_id, name, created_at, updated_at) VALUES ('pl1', 'p1', 'main', 0, 0)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        // b1 is the oldest; each build has one 100-byte artifact.
        for number in 1..=4_i64 {
            let finished_at = number * DAY;
            sqlx::query(
                "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
                 queued_at, finished_at, created_at, updated_at) \
                 VALUES (?1, 'p1', 'pl1', ?2, 'succeeded', 'manual', ?3, ?3, ?3, ?3)",
            )
            .bind(format!("b{number}"))
            .bind(number)
            .bind(finished_at)
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO artifacts (id, build_id, name, file_path, file_size, expires_at, created_at) \
                 VALUES (?1, ?2, 'app.aab', ?3, 100, ?4, ?4)",
            )
            .bind(format!("a{number}"))
            .bind(format!("b{number}"))
            .bind(format!("artifacts/b{number}/app.aab"))
            .bind(finished_at)
            .execute(&pool)
            .await
            .unwrap();
        }
        pool
    }

    async fn add_release(pool: &SqlitePool, build_id: &str, state: &str) {
        sqlx::query(
            "INSERT INTO releases (id, project_id, build_id, artifact_id, store, track, state, \
             rollout_fraction, created_at, updated_at) \
             VALUES (?1, 'p1', ?2, NULL, 'google_play', 'production', ?3, 0.2, 0, 0)",
        )
        .bind(format!("r-{build_id}"))
        .bind(build_id)
        .bind(state)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO release_history (id, release_id, to_state, at) VALUES (?1, ?2, ?3, 0)",
        )
        .bind(format!("h-{build_id}"))
        .bind(format!("r-{build_id}"))
        .bind(state)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn set_policy(pool: &SqlitePool, limit: &str, target: &str) {
        let (age, count, size) = match limit {
            "age" => (Some(30), None, None),
            "count" => (None, Some(1), None),
            "size" => (None, None, Some(100)),
            _ => unreachable!(),
        };
        sqlx::query(
            "INSERT OR REPLACE INTO retention_policy (id, enabled, max_age_days, \
             max_builds_per_project, max_artifact_size_bytes, cleanup_target, created_at, updated_at) \
             VALUES (1, 1, ?1, ?2, ?3, ?4, 0, 0)",
        )
        .bind(age)
        .bind(count)
        .bind(size)
        .bind(target)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn cleanup(pool: &SqlitePool) {
        let mut totals = CleanupTotals::default();
        cleanup_project(pool, &StorageBackend::Disabled, "p1", NOW, &mut totals)
            .await
            .unwrap();
    }

    async fn count(pool: &SqlitePool, sql: &'static str, id: &str) -> i64 {
        sqlx::query_scalar(sql)
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn build_status(pool: &SqlitePool, build_id: &str) -> Option<String> {
        sqlx::query_scalar("SELECT status FROM builds WHERE id = ?1")
            .bind(build_id)
            .fetch_optional(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn retention_keeps_a_build_whose_release_is_in_progress() {
        for limit in ["age", "count", "size"] {
            for target in ["artifacts_only", "full"] {
                let pool = pool().await;
                add_release(&pool, "b1", "rolling").await;
                set_policy(&pool, limit, target).await;

                cleanup(&pool).await;

                let case = format!("{limit} limit, {target} cleanup");
                assert_eq!(
                    build_status(&pool, "b1").await.as_deref(),
                    Some("succeeded"),
                    "{case}"
                );
                assert_eq!(
                    count(
                        &pool,
                        "SELECT COUNT(*) FROM artifacts WHERE build_id = ?1",
                        "b1"
                    )
                    .await,
                    1,
                    "{case}"
                );
                assert_eq!(
                    count(
                        &pool,
                        "SELECT COUNT(*) FROM releases WHERE build_id = ?1",
                        "b1"
                    )
                    .await,
                    1,
                    "{case}"
                );
                assert_eq!(
                    count(
                        &pool,
                        "SELECT COUNT(*) FROM release_history WHERE release_id = ?1",
                        "r-b1"
                    )
                    .await,
                    1,
                    "{case}"
                );
                // Retention still cleans the build without a release.
                let b2 = build_status(&pool, "b2").await;
                assert!(
                    b2.is_none() || b2.as_deref() == Some("expired"),
                    "{case}: b2 is {b2:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn full_cleanup_keeps_the_record_of_a_finished_release() {
        let pool = pool().await;
        add_release(&pool, "b1", "complete").await;
        set_policy(&pool, "count", "full").await;

        cleanup(&pool).await;

        // The finished release's build loses its artifacts but keeps its row.
        assert_eq!(build_status(&pool, "b1").await.as_deref(), Some("expired"));
        assert_eq!(
            count(
                &pool,
                "SELECT COUNT(*) FROM artifacts WHERE build_id = ?1",
                "b1"
            )
            .await,
            0
        );
        assert_eq!(
            count(
                &pool,
                "SELECT COUNT(*) FROM releases WHERE build_id = ?1",
                "b1"
            )
            .await,
            1
        );
        assert_eq!(build_status(&pool, "b2").await, None);

        // Later runs leave it alone.
        cleanup(&pool).await;
        assert_eq!(build_status(&pool, "b1").await.as_deref(), Some("expired"));
    }

    #[tokio::test]
    async fn artifact_expiry_skips_builds_with_a_release_in_progress() {
        let pool = pool().await;
        add_release(&pool, "b1", "rolling").await;
        add_release(&pool, "b2", "complete").await;

        let expired: Vec<String> = load_expired_artifacts(&pool, NOW)
            .await
            .unwrap()
            .iter()
            .map(|row| row.get("build_id"))
            .collect();

        assert_eq!(expired, ["b2", "b3", "b4"]);
    }

    async fn statuses(pool: &SqlitePool) -> Vec<(String, String, Option<String>)> {
        sqlx::query_as("SELECT id, status, expired_from FROM builds ORDER BY build_number")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    fn row(id: &str, status: &str, expired_from: Option<&str>) -> (String, String, Option<String>) {
        (id.into(), status.into(), expired_from.map(Into::into))
    }

    #[tokio::test]
    async fn expired_builds_keep_their_outcome_and_full_cleanup_removes_them() {
        let pool = pool().await;
        sqlx::query("UPDATE builds SET status = 'failed' WHERE id = 'b2'")
            .execute(&pool)
            .await
            .unwrap();
        set_policy(&pool, "count", "artifacts_only").await;
        // count keeps one build; raise it so b4 and b3 stay.
        sqlx::query("UPDATE retention_policy SET max_builds_per_project = 2")
            .execute(&pool)
            .await
            .unwrap();

        cleanup(&pool).await;
        assert_eq!(
            statuses(&pool).await,
            [
                row("b1", "expired", Some("succeeded")),
                row("b2", "expired", Some("failed")),
                row("b3", "succeeded", None),
                row("b4", "succeeded", None),
            ]
        );

        sqlx::query("UPDATE retention_policy SET cleanup_target = 'full'")
            .execute(&pool)
            .await
            .unwrap();
        cleanup(&pool).await;
        assert_eq!(
            statuses(&pool).await,
            [row("b3", "succeeded", None), row("b4", "succeeded", None)]
        );
    }

    #[tokio::test]
    async fn keep_statuses_still_protect_expired_builds_under_full_cleanup() {
        let pool = pool().await;
        sqlx::query(
            "UPDATE builds SET status = 'expired', expired_from = 'succeeded' WHERE id IN ('b1', 'b2')",
        )
        .execute(&pool)
        .await
        .unwrap();
        set_policy(&pool, "count", "full").await;
        sqlx::query("UPDATE retention_policy SET keep_statuses = '[\"succeeded\"]'")
            .execute(&pool)
            .await
            .unwrap();

        cleanup(&pool).await;

        assert_eq!(statuses(&pool).await.len(), 4);
    }

    #[tokio::test]
    async fn size_limit_ignores_uploads_that_hold_no_file() {
        let pool = pool().await;
        sqlx::query(
            "INSERT INTO artifacts (id, build_id, name, file_path, file_size, state, created_at) \
             VALUES ('pending', 'b4', 'big.ipa', 'artifacts/b4/big.ipa', 10000, 'pending', 0), \
                    ('failed', 'b4', 'big2.ipa', 'artifacts/b4/big2.ipa', 10000, 'failed', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        set_policy(&pool, "size", "artifacts_only").await;
        sqlx::query("UPDATE retention_policy SET max_artifact_size_bytes = 400")
            .execute(&pool)
            .await
            .unwrap();

        cleanup(&pool).await;

        assert!(
            statuses(&pool)
                .await
                .iter()
                .all(|(_, status, _)| status == "succeeded")
        );
    }

    #[tokio::test]
    async fn size_limit_removes_only_what_protected_builds_leave_room_for() {
        for (max_size, expected) in [
            // b2..b4 are protected and already over the limit: nothing helps.
            (200, vec!["failed", "succeeded", "succeeded", "succeeded"]),
            // Removing b1 alone reaches the limit.
            (300, vec!["expired", "succeeded", "succeeded", "succeeded"]),
        ] {
            let pool = pool().await;
            sqlx::query("UPDATE builds SET status = 'failed' WHERE id = 'b1'")
                .execute(&pool)
                .await
                .unwrap();
            // A newer failed build must survive too.
            sqlx::query(
                "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
                 queued_at, finished_at, created_at, updated_at) \
                 VALUES ('b5', 'p1', 'pl1', 5, 'failed', 'manual', ?1, ?1, ?1, ?1)",
            )
            .bind(NOW - 60)
            .execute(&pool)
            .await
            .unwrap();
            set_policy(&pool, "size", "artifacts_only").await;
            sqlx::query(
                "UPDATE retention_policy SET max_artifact_size_bytes = ?1, \
                 keep_statuses = '[\"succeeded\"]'",
            )
            .bind(max_size)
            .execute(&pool)
            .await
            .unwrap();

            cleanup(&pool).await;

            let mut got: Vec<String> = statuses(&pool)
                .await
                .into_iter()
                .map(|(_, status, _)| status)
                .collect();
            assert_eq!(got.pop().as_deref(), Some("failed"), "b5 at {max_size}");
            assert_eq!(got, expected, "max size {max_size}");
        }
    }

    #[tokio::test]
    async fn full_cleanup_removes_the_screenshots_of_deleted_builds_only() {
        let pool = pool().await;
        let dir = tempfile::tempdir().unwrap();
        let storage = StorageBackend::Local(Arc::new(
            crate::storage::LocalStorageClient::new(dir.path().to_path_buf(), None).unwrap(),
        ));
        // b1 is deleted outright; b2 keeps its row for a finished release.
        add_release(&pool, "b2", "complete").await;
        for build_id in ["b1", "b2"] {
            let key = format!("feedback/p1/{build_id}/f.png");
            storage
                .put_object(&key, b"\x89PNG\r\n\x1a\n".to_vec(), "image/png")
                .await
                .unwrap();
            sqlx::query(
                "INSERT INTO build_feedback (id, build_id, author_user_id, author_email, message, \
                 screenshot_key, created_at) VALUES (?1, ?2, 'u1', 'qa@example.com', 'hi', ?3, 0)",
            )
            .bind(format!("f-{build_id}"))
            .bind(build_id)
            .bind(&key)
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT OR REPLACE INTO retention_policy (id, enabled, max_builds_per_project, \
             cleanup_target, created_at, updated_at) VALUES (1, 1, 2, 'full', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let mut totals = CleanupTotals::default();
        cleanup_project(&pool, &storage, "p1", NOW, &mut totals)
            .await
            .unwrap();

        assert_eq!(build_status(&pool, "b1").await, None);
        assert!(!dir.path().join("feedback/p1/b1/f.png").exists());
        assert_eq!(build_status(&pool, "b2").await.as_deref(), Some("expired"));
        assert!(dir.path().join("feedback/p1/b2/f.png").exists());
        assert_eq!(
            count(
                &pool,
                "SELECT COUNT(*) FROM build_feedback WHERE build_id = ?1",
                "b2"
            )
            .await,
            1
        );
    }

    #[tokio::test]
    async fn stale_upload_sweep_fails_the_row_before_deleting_the_file() {
        let pool = pool().await;
        let dir = tempfile::tempdir().unwrap();
        let storage = StorageBackend::Local(Arc::new(
            crate::storage::LocalStorageClient::new(dir.path().to_path_buf(), None).unwrap(),
        ));
        let stale_at =
            NOW - crate::artifacts::UPLOAD_URL_TTL_SECS as i64 - STALE_PENDING_GRACE_SECS - 1;
        for (id, created_at) in [("late", stale_at), ("recent", NOW - 60)] {
            let file_path = format!("artifacts/b4/{id}.ipa");
            std::fs::create_dir_all(dir.path().join("artifacts/b4")).unwrap();
            std::fs::write(dir.path().join(&file_path), b"ipa").unwrap();
            sqlx::query(
                "INSERT INTO artifacts (id, build_id, name, file_path, state, created_at) \
                 VALUES (?1, 'b4', ?1, ?2, 'pending', ?3)",
            )
            .bind(id)
            .bind(&file_path)
            .bind(created_at)
            .execute(&pool)
            .await
            .unwrap();
        }

        sweep_stale_pending_artifacts(&pool, &storage, NOW)
            .await
            .unwrap();

        let state = |id: &'static str| {
            sqlx::query_scalar::<_, String>("SELECT state FROM artifacts WHERE id = ?1")
                .bind(id)
                .fetch_one(&pool)
        };
        assert_eq!(state("late").await.unwrap(), "failed");
        assert!(!dir.path().join("artifacts/b4/late.ipa").exists());
        assert_eq!(state("recent").await.unwrap(), "pending");
        assert!(dir.path().join("artifacts/b4/recent.ipa").exists());

        // A runner finishing the swept upload now loses the race cleanly.
        let completed = sqlx::query(
            "UPDATE artifacts SET state = 'available' WHERE id = 'late' AND state = 'pending'",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(completed.rows_affected(), 0);
    }
}
