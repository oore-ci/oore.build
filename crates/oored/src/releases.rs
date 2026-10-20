//! Store releases: the tracked record from upload to 100% rollout.
//!
//! Every state or fraction change goes through [`transition_release`], which
//! applies the change at the store first, then writes the release row, a
//! history entry and an audit log entry, and publishes a release event.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use oore_contract::{
    ApiError, CreateReleaseRequest, ListReleaseActivityResponse, ListReleasesResponse, ProjectRole,
    Release, ReleaseAction, ReleaseActionRequest, ReleaseActivityEvent, ReleaseApprovalGate,
    ReleaseDetailResponse, ReleaseHealth, ReleaseHistoryEntry, ReleasePolicy, ReleaseState,
    ReleaseStore,
};
use serde::Deserialize;
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::project_rbac::{
    EffectiveProjectRole, ProjectPermission, check_project_permission, effective_member_role,
    require_project_permission, resolve_effective_project_role,
};
use crate::scheduler::{ReleaseEvent, ReleaseEventKind, Scheduler};
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Minimum seconds between crash-rate checks of one release.
pub(crate) const HEALTH_CHECK_INTERVAL_SECS: i64 = 15 * 60;
const MAX_REASON_LEN: usize = 500;

// ── Store seam ──────────────────────────────────────────────────

/// A change the store must apply before Oore records it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StoreChange {
    Rollout { fraction: f64 },
    Pause,
    Resume { fraction: f64 },
    Halt,
    Complete,
}

/// Outbound store calls made from the daemon host.
pub trait ReleaseStoreClient: Send + Sync {
    fn apply<'a>(
        &'a self,
        release: &'a Release,
        change: StoreChange,
    ) -> BoxFuture<'a, anyhow::Result<()>>;

    /// Current crash rate (0..1) for the release, if the store reports one.
    fn crash_rate<'a>(&'a self, release: &'a Release)
    -> BoxFuture<'a, anyhow::Result<Option<f64>>>;
}

/// Records changes without calling a store. Used until a store client is configured.
pub struct NoopReleaseStoreClient;

impl ReleaseStoreClient for NoopReleaseStoreClient {
    fn apply<'a>(&'a self, _: &'a Release, _: StoreChange) -> BoxFuture<'a, anyhow::Result<()>> {
        Box::pin(async { Ok(()) })
    }

    fn crash_rate<'a>(&'a self, _: &'a Release) -> BoxFuture<'a, anyhow::Result<Option<f64>>> {
        Box::pin(async { Ok(None) })
    }
}

/// What release operations need from the daemon.
#[derive(Clone, Copy)]
pub struct ReleaseDeps<'a> {
    pub pool: &'a SqlitePool,
    pub scheduler: &'a Scheduler,
    pub store: &'a dyn ReleaseStoreClient,
}

impl<'a> From<&'a AppState> for ReleaseDeps<'a> {
    fn from(state: &'a AppState) -> Self {
        Self {
            pool: &state.db,
            scheduler: &state.scheduler,
            store: state.release_store.as_ref(),
        }
    }
}

// ── Domain ──────────────────────────────────────────────────────

/// Who made a change. `System` is the daemon or a store callback.
#[derive(Debug, Clone)]
pub enum ReleaseActor {
    System,
    User { user_id: String, email: String },
}

impl ReleaseActor {
    fn user_id(&self) -> Option<&str> {
        match self {
            Self::System => None,
            Self::User { user_id, .. } => Some(user_id),
        }
    }

    fn email(&self) -> Option<&str> {
        match self {
            Self::System => None,
            Self::User { email, .. } => Some(email),
        }
    }
}

#[derive(Debug)]
pub enum ReleaseError {
    NotFound,
    Invalid(String),
    /// The release is not in a state that allows the change.
    Conflict(String),
    Store(anyhow::Error),
    Db(anyhow::Error),
}

impl From<sqlx::Error> for ReleaseError {
    fn from(error: sqlx::Error) -> Self {
        Self::Db(error.into())
    }
}

impl ReleaseError {
    pub fn into_api(self) -> (StatusCode, Json<ApiError>) {
        match self {
            ReleaseError::NotFound => {
                api_err(StatusCode::NOT_FOUND, "not_found", "Release not found")
            }
            ReleaseError::Invalid(message) => {
                api_err(StatusCode::BAD_REQUEST, "invalid_input", message)
            }
            ReleaseError::Conflict(message) => {
                api_err(StatusCode::CONFLICT, "invalid_release_state", message)
            }
            ReleaseError::Store(error) => {
                warn!(error = %error, "store rejected release change");
                api_err(
                    StatusCode::BAD_GATEWAY,
                    "store_rejected",
                    "The store did not accept the change",
                )
            }
            ReleaseError::Db(error) => {
                error!(error = %error, "release store error");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to update release",
                )
            }
        }
    }
}

pub struct NewRelease {
    pub project_id: String,
    pub build_id: String,
    pub artifact_id: Option<String>,
    pub store: ReleaseStore,
    pub track: String,
    pub package_name: Option<String>,
    pub version_name: Option<String>,
    pub version_code: Option<i64>,
    pub state: ReleaseState,
    pub rollout_fraction: f64,
    pub policy: ReleasePolicy,
}

/// A requested state or fraction change.
pub struct ReleaseChange {
    pub to_state: ReleaseState,
    /// `None` keeps the current fraction.
    pub to_fraction: Option<f64>,
    pub audit_action: &'static str,
}

pub(crate) fn awaiting_approval(release: &Release) -> bool {
    if release.approved_at.is_some() || release.state.is_terminal() {
        return false;
    }
    match release.policy.approval {
        ReleaseApprovalGate::None => false,
        ReleaseApprovalGate::BeforeUpload => release.state == ReleaseState::Uploading,
        ReleaseApprovalGate::BeforeProduction => release.track == "production",
    }
}

/// When the daemon should take the next step, if it drives this release.
fn next_step_at(release: &Release, now: i64) -> Option<i64> {
    let driven = release.store == ReleaseStore::GooglePlay
        && release.state == ReleaseState::Rolling
        && !awaiting_approval(release)
        && release.policy.next_step(release.rollout_fraction).is_some();
    driven.then(|| now + release.policy.interval_secs())
}

fn store_change(from: ReleaseState, to: ReleaseState, fraction: f64) -> Option<StoreChange> {
    use ReleaseState::{Complete, Halted, Paused, Rolling};
    match (from, to) {
        (Rolling, Rolling) => Some(StoreChange::Rollout { fraction }),
        (Paused | Halted, Rolling) => Some(StoreChange::Resume { fraction }),
        (_, Paused) => Some(StoreChange::Pause),
        (_, Halted) => Some(StoreChange::Halt),
        (Rolling | Paused, Complete) => Some(StoreChange::Complete),
        // Upload and review progress is reported by the store, not sent to it.
        _ => None,
    }
}

fn event_kind(from: ReleaseState, to: ReleaseState) -> ReleaseEventKind {
    use ReleaseState::{Complete, Failed, Halted, Paused, Rolling};
    match (from, to) {
        (_, Failed) => ReleaseEventKind::Failed,
        (_, Complete) => ReleaseEventKind::RolloutCompleted,
        (_, Halted) => ReleaseEventKind::RolloutHalted,
        (_, Paused) => ReleaseEventKind::RolloutPaused,
        (Rolling, Rolling) => ReleaseEventKind::RolloutStepRaised,
        (Paused | Halted, Rolling) => ReleaseEventKind::RolloutResumed,
        _ => ReleaseEventKind::Uploaded,
    }
}

/// Actions a caller with `role` may take on `release` right now.
pub(crate) fn allowed_actions(
    role: &EffectiveProjectRole,
    release: &Release,
) -> Vec<ReleaseAction> {
    let operate = check_project_permission(role, ProjectPermission::OperateRelease);
    let manage = check_project_permission(role, ProjectPermission::ManageReleases);
    let awaiting = awaiting_approval(release);
    let state = release.state;
    let mut actions = Vec::new();
    if manage && state == ReleaseState::Rolling && !awaiting && release.rollout_fraction < 1.0 {
        actions.push(ReleaseAction::Raise);
    }
    if operate && state == ReleaseState::Rolling {
        actions.push(ReleaseAction::Pause);
    }
    if manage && matches!(state, ReleaseState::Paused | ReleaseState::Halted) && !awaiting {
        actions.push(ReleaseAction::Resume);
    }
    if operate && matches!(state, ReleaseState::Rolling | ReleaseState::Paused) {
        actions.push(ReleaseAction::Halt);
    }
    if manage && matches!(state, ReleaseState::Rolling | ReleaseState::Paused) && !awaiting {
        actions.push(ReleaseAction::Complete);
    }
    if manage && awaiting {
        actions.push(ReleaseAction::Approve);
    }
    actions
}

const RELEASE_SELECT: &str = "SELECT r.*, p.name AS project_name, b.build_number AS build_number, \
     au.email AS approved_by_email \
     FROM releases r \
     LEFT JOIN projects p ON p.id = r.project_id \
     LEFT JOIN builds b ON b.id = r.build_id \
     LEFT JOIN users au ON au.id = r.approved_by";

fn row_to_release(row: &SqliteRow) -> Result<Release, ReleaseError> {
    let parse_err = |message: String| ReleaseError::Db(anyhow::anyhow!(message));
    let store: ReleaseStore = row.get::<String, _>("store").parse().map_err(parse_err)?;
    let state: ReleaseState = row.get::<String, _>("state").parse().map_err(parse_err)?;
    let policy: ReleasePolicy = serde_json::from_str(&row.get::<String, _>("policy_json"))
        .map_err(|error| ReleaseError::Db(error.into()))?;
    let rollout_fraction: f64 = row.get("rollout_fraction");
    let crash_rate: Option<f64> = row.get("crash_rate");
    let mut release = Release {
        id: row.get("id"),
        project_id: row.get("project_id"),
        project_name: row.get("project_name"),
        build_id: row.get("build_id"),
        build_number: row.get("build_number"),
        artifact_id: row.get("artifact_id"),
        platform: store.platform().to_string(),
        store,
        track: row.get("track"),
        package_name: row.get("package_name"),
        version_name: row.get("version_name"),
        version_code: row.get("version_code"),
        state,
        rollout_fraction,
        next_fraction: None,
        next_step_at: row.get("next_step_at"),
        health: ReleaseHealth::from_crash_rate(crash_rate, policy.crash_rate_threshold),
        crash_rate,
        crash_rate_threshold: policy.crash_rate_threshold,
        policy,
        awaiting_approval: false,
        approved_at: row.get("approved_at"),
        approved_by_email: row.get("approved_by_email"),
        allowed_actions: Vec::new(),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    };
    release.awaiting_approval = awaiting_approval(&release);
    if !state.is_terminal() {
        release.next_fraction = release.policy.next_step(rollout_fraction);
    }
    Ok(release)
}

pub(crate) async fn load_release(
    pool: &SqlitePool,
    release_id: &str,
) -> Result<Release, ReleaseError> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{RELEASE_SELECT} WHERE r.id = ?1"
    )))
    .bind(release_id)
    .fetch_optional(pool)
    .await?
    .ok_or(ReleaseError::NotFound)?;
    row_to_release(&row)
}

fn publish(
    scheduler: &Scheduler,
    release: &Release,
    kind: ReleaseEventKind,
    actor: &ReleaseActor,
    reason: Option<&str>,
) {
    scheduler.publish_release_event(ReleaseEvent {
        kind,
        project_id: release.project_id.clone(),
        release_id: release.id.clone(),
        build_id: Some(release.build_id.clone()),
        version_name: release.version_name.clone(),
        version_code: release.version_code,
        track: Some(release.track.clone()),
        rollout_percent: Some(release.rollout_fraction * 100.0),
        actor: actor.email().map(str::to_string),
        reason: reason.map(str::to_string),
        timestamp: now_unix(),
    });
}

async fn audit(
    pool: &SqlitePool,
    action: &str,
    release_id: &str,
    actor: &ReleaseActor,
    details: serde_json::Value,
) {
    if let Err(error) = write_audit_log(
        pool,
        actor.user_id(),
        action,
        "release",
        Some(release_id),
        Some(&details.to_string()),
    )
    .await
    {
        warn!(release_id, error = %error, "failed to write release audit log");
    }
}

fn validate_fraction(fraction: f64) -> Result<(), ReleaseError> {
    if fraction.is_finite() && (0.0..=1.0).contains(&fraction) {
        Ok(())
    } else {
        Err(ReleaseError::Invalid(
            "fraction must be between 0 and 1".into(),
        ))
    }
}

fn clean_reason(reason: Option<&str>) -> Option<String> {
    reason
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
        .map(|reason| reason.chars().take(MAX_REASON_LEN).collect())
}

/// Create a release record with its first history entry.
pub async fn create_release(
    deps: ReleaseDeps<'_>,
    input: NewRelease,
    actor: &ReleaseActor,
    reason: Option<&str>,
) -> Result<Release, ReleaseError> {
    input.policy.validate().map_err(ReleaseError::Invalid)?;
    validate_fraction(input.rollout_fraction)?;
    if input.track.trim().is_empty() {
        return Err(ReleaseError::Invalid("track must not be empty".into()));
    }
    // `complete` covers an upload straight to 100%, `paused` a draft at 0%.
    if matches!(input.state, ReleaseState::Halted | ReleaseState::Failed) {
        return Err(ReleaseError::Invalid(
            "a release cannot start halted or failed".into(),
        ));
    }
    let reason = clean_reason(reason);
    let now = now_unix();
    let id = Uuid::new_v4().to_string();
    let policy_json =
        serde_json::to_string(&input.policy).map_err(|error| ReleaseError::Db(error.into()))?;

    let mut tx = deps.pool.begin().await?;
    sqlx::query(
        "INSERT INTO releases (id, project_id, build_id, artifact_id, store, track, package_name, \
         version_name, version_code, state, rollout_fraction, policy_json, created_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)",
    )
    .bind(&id)
    .bind(&input.project_id)
    .bind(&input.build_id)
    .bind(&input.artifact_id)
    .bind(input.store.as_str())
    .bind(input.track.trim())
    .bind(&input.package_name)
    .bind(&input.version_name)
    .bind(input.version_code)
    .bind(input.state.as_str())
    .bind(input.rollout_fraction)
    .bind(&policy_json)
    .bind(actor.user_id())
    .bind(now)
    .execute(&mut *tx)
    .await?;
    insert_history(
        &mut tx,
        &id,
        actor,
        None,
        input.state,
        None,
        Some(input.rollout_fraction),
        reason.as_deref(),
        now,
    )
    .await?;
    tx.commit().await?;

    let mut release = load_release(deps.pool, &id).await?;
    if let Some(at) = next_step_at(&release, now) {
        sqlx::query("UPDATE releases SET next_step_at = ?1 WHERE id = ?2")
            .bind(at)
            .bind(&id)
            .execute(deps.pool)
            .await?;
        release.next_step_at = Some(at);
    }

    audit(
        deps.pool,
        "release_created",
        &id,
        actor,
        serde_json::json!({
            "actor_email": actor.email(),
            "store": release.store,
            "track": release.track,
            "state": release.state,
            "rollout_fraction": release.rollout_fraction,
            "reason": reason,
        }),
    )
    .await;
    publish(
        deps.scheduler,
        &release,
        ReleaseEventKind::Created,
        actor,
        reason.as_deref(),
    );
    if matches!(
        release.state,
        ReleaseState::Rolling | ReleaseState::Complete
    ) {
        publish(
            deps.scheduler,
            &release,
            ReleaseEventKind::Uploaded,
            actor,
            reason.as_deref(),
        );
    }
    if release.awaiting_approval {
        publish(
            deps.scheduler,
            &release,
            ReleaseEventKind::ApprovalNeeded,
            actor,
            reason.as_deref(),
        );
    }
    info!(release_id = %id, store = %release.store, track = %release.track, "release created");
    Ok(release)
}

#[allow(clippy::too_many_arguments)]
async fn insert_history(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    release_id: &str,
    actor: &ReleaseActor,
    from_state: Option<ReleaseState>,
    to_state: ReleaseState,
    from_fraction: Option<f64>,
    to_fraction: Option<f64>,
    reason: Option<&str>,
    at: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO release_history (id, release_id, actor_id, actor_email, from_state, to_state, \
         from_fraction, to_fraction, reason, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(release_id)
    .bind(actor.user_id())
    .bind(actor.email())
    .bind(from_state.map(ReleaseState::as_str))
    .bind(to_state.as_str())
    .bind(from_fraction)
    .bind(to_fraction)
    .bind(reason)
    .bind(at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Move a release to a new state or fraction.
///
/// The store applies the change first; the row only changes if it is still in
/// the state that was read, so concurrent changes cannot both land.
pub async fn transition_release(
    deps: ReleaseDeps<'_>,
    release_id: &str,
    change: ReleaseChange,
    actor: &ReleaseActor,
    reason: Option<&str>,
) -> Result<Release, ReleaseError> {
    let current = load_release(deps.pool, release_id).await?;
    let from = current.state;
    let to = change.to_state;
    if !from.can_transition_to(to) {
        return Err(ReleaseError::Conflict(format!(
            "release is {from} and cannot move to {to}"
        )));
    }
    let to_fraction = match to {
        ReleaseState::Complete => 1.0,
        _ => change.to_fraction.unwrap_or(current.rollout_fraction),
    };
    validate_fraction(to_fraction)?;
    if from == ReleaseState::Rolling
        && to == ReleaseState::Rolling
        && to_fraction <= current.rollout_fraction
    {
        return Err(ReleaseError::Invalid(
            "rollout fraction can only increase".into(),
        ));
    }
    if awaiting_approval(&current)
        && matches!(
            from,
            ReleaseState::Rolling | ReleaseState::Paused | ReleaseState::Halted
        )
        && matches!(to, ReleaseState::Rolling | ReleaseState::Complete)
    {
        return Err(ReleaseError::Conflict(
            "release is waiting for approval".into(),
        ));
    }
    let reason = clean_reason(reason);

    if let Some(store_change) = store_change(from, to, to_fraction) {
        deps.store
            .apply(&current, store_change)
            .await
            .map_err(ReleaseError::Store)?;
    }

    let now = now_unix();
    let mut next = current.clone();
    next.state = to;
    next.rollout_fraction = to_fraction;
    let next_at = next_step_at(&next, now);

    let mut tx = deps.pool.begin().await?;
    let updated = sqlx::query(
        "UPDATE releases SET state = ?1, rollout_fraction = ?2, next_step_at = ?3, updated_at = ?4 \
         WHERE id = ?5 AND state = ?6 AND rollout_fraction = ?7",
    )
    .bind(to.as_str())
    .bind(to_fraction)
    .bind(next_at)
    .bind(now)
    .bind(release_id)
    .bind(from.as_str())
    .bind(current.rollout_fraction)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(ReleaseError::Conflict(
            "release changed, reload and try again".into(),
        ));
    }
    insert_history(
        &mut tx,
        release_id,
        actor,
        Some(from),
        to,
        Some(current.rollout_fraction),
        Some(to_fraction),
        reason.as_deref(),
        now,
    )
    .await?;
    tx.commit().await?;

    let release = load_release(deps.pool, release_id).await?;
    audit(
        deps.pool,
        change.audit_action,
        release_id,
        actor,
        serde_json::json!({
            "actor_email": actor.email(),
            "from_state": from,
            "to_state": to,
            "from_fraction": current.rollout_fraction,
            "to_fraction": to_fraction,
            "reason": reason,
        }),
    )
    .await;
    publish(
        deps.scheduler,
        &release,
        event_kind(from, to),
        actor,
        reason.as_deref(),
    );
    info!(release_id, from = %from, to = %to, fraction = to_fraction, "release transitioned");
    Ok(release)
}

/// Record a maintainer approval and start the schedule if the release is rolling.
pub async fn approve_release(
    deps: ReleaseDeps<'_>,
    release_id: &str,
    actor: &ReleaseActor,
    reason: Option<&str>,
) -> Result<Release, ReleaseError> {
    let current = load_release(deps.pool, release_id).await?;
    if !current.awaiting_approval {
        return Err(ReleaseError::Conflict(
            "release is not waiting for approval".into(),
        ));
    }
    let reason = clean_reason(reason);
    let now = now_unix();
    let mut approved = current.clone();
    approved.approved_at = Some(now);
    // A draft release starts at the next tick; a live one waits a full interval.
    let next_at = next_step_at(&approved, now).map(|at| {
        if current.rollout_fraction == 0.0 {
            now
        } else {
            at
        }
    });

    let mut tx = deps.pool.begin().await?;
    let updated = sqlx::query(
        "UPDATE releases SET approved_at = ?1, approved_by = ?2, next_step_at = ?3, updated_at = ?1 \
         WHERE id = ?4 AND approved_at IS NULL AND state = ?5",
    )
    .bind(now)
    .bind(actor.user_id())
    .bind(next_at)
    .bind(release_id)
    .bind(current.state.as_str())
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(ReleaseError::Conflict(
            "release changed, reload and try again".into(),
        ));
    }
    insert_history(
        &mut tx,
        release_id,
        actor,
        Some(current.state),
        current.state,
        Some(current.rollout_fraction),
        Some(current.rollout_fraction),
        Some(reason.as_deref().unwrap_or("approved")),
        now,
    )
    .await?;
    tx.commit().await?;

    let release = load_release(deps.pool, release_id).await?;
    audit(
        deps.pool,
        "release_approved",
        release_id,
        actor,
        serde_json::json!({
            "actor_email": actor.email(),
            "state": release.state,
            "gate": release.policy.approval,
            "reason": reason,
        }),
    )
    .await;
    publish(
        deps.scheduler,
        &release,
        ReleaseEventKind::Approved,
        actor,
        reason.as_deref(),
    );
    Ok(release)
}

/// Whether the runner may upload for this release (`before_upload` gate).
pub async fn upload_approved(pool: &SqlitePool, release_id: &str) -> Result<bool, ReleaseError> {
    let release = load_release(pool, release_id).await?;
    Ok(!release.awaiting_approval)
}

// ── Rollout schedule ────────────────────────────────────────────

/// One pass of the rollout monitor: check health, halt on breach, take due steps.
///
/// Only rolling Google Play releases are driven; App Store phased release
/// follows Apple's curve. Halted and paused releases are never selected.
pub(crate) async fn run_rollout_tick(
    deps: ReleaseDeps<'_>,
    now: i64,
    batch: i64,
) -> anyhow::Result<()> {
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{RELEASE_SELECT} WHERE r.state = 'rolling' AND r.store = 'google_play' \
         ORDER BY COALESCE(r.next_step_at, 0) LIMIT ?1"
    )))
    .bind(batch)
    .fetch_all(deps.pool)
    .await?;

    for row in rows {
        let mut release = match row_to_release(&row) {
            Ok(release) => release,
            Err(error) => {
                warn!(error = ?error, "rollout monitor: unreadable release row");
                continue;
            }
        };
        let checked_at: Option<i64> = row.get("crash_rate_checked_at");

        if let Some(threshold) = release.policy.crash_rate_threshold
            && checked_at.is_none_or(|at| now - at >= HEALTH_CHECK_INTERVAL_SECS)
        {
            match deps.store.crash_rate(&release).await {
                Ok(rate) => {
                    sqlx::query(
                        "UPDATE releases SET crash_rate = COALESCE(?1, crash_rate), crash_rate_checked_at = ?2 \
                         WHERE id = ?3",
                    )
                    .bind(rate)
                    .bind(now)
                    .bind(&release.id)
                    .execute(deps.pool)
                    .await?;
                    if let Some(rate) = rate {
                        release.crash_rate = Some(rate);
                        if rate >= threshold {
                            let reason = format!(
                                "crash rate {:.2}% reached the {:.2}% threshold",
                                rate * 100.0,
                                threshold * 100.0
                            );
                            if let Err(error) = transition_release(
                                deps,
                                &release.id,
                                ReleaseChange {
                                    to_state: ReleaseState::Halted,
                                    to_fraction: None,
                                    audit_action: "release_halted",
                                },
                                &ReleaseActor::System,
                                Some(&reason),
                            )
                            .await
                            {
                                warn!(release_id = %release.id, error = ?error, "rollout monitor: halt failed");
                            }
                            continue;
                        }
                    }
                }
                Err(error) => {
                    warn!(release_id = %release.id, error = %error, "rollout monitor: crash rate check failed");
                }
            }
        }

        let due = release.next_step_at.is_some_and(|at| at <= now);
        if !due || release.awaiting_approval {
            continue;
        }
        let Some(fraction) = release.policy.next_step(release.rollout_fraction) else {
            continue;
        };
        let to_state = if fraction >= 1.0 {
            ReleaseState::Complete
        } else {
            ReleaseState::Rolling
        };
        if let Err(error) = transition_release(
            deps,
            &release.id,
            ReleaseChange {
                to_state,
                to_fraction: Some(fraction),
                audit_action: "release_advanced",
            },
            &ReleaseActor::System,
            Some("scheduled rollout step"),
        )
        .await
        {
            warn!(release_id = %release.id, error = ?error, "rollout monitor: step failed");
        }
    }
    Ok(())
}

// ── HTTP handlers ───────────────────────────────────────────────

fn user_actor(auth: &AuthUser) -> ReleaseActor {
    ReleaseActor::User {
        user_id: auth.0.user_id.clone(),
        email: auth.0.email.clone(),
    }
}

async fn project_role(
    state: &AppState,
    auth: &AuthUser,
    project_id: &str,
) -> Result<EffectiveProjectRole, (StatusCode, Json<ApiError>)> {
    resolve_effective_project_role(
        &state.db,
        &auth.0.user_id,
        &auth.0.role,
        project_id,
        &auth.0.auth_source,
    )
    .await
}

fn row_role(auth: &AuthUser, row: &SqliteRow) -> EffectiveProjectRole {
    if auth.0.role == "owner" || auth.0.role == "admin" {
        return EffectiveProjectRole::InstanceAdmin;
    }
    row.get::<Option<String>, _>("member_role")
        .and_then(|role| role.parse::<ProjectRole>().ok())
        .map_or(EffectiveProjectRole::None, |role| {
            EffectiveProjectRole::Member(effective_member_role(
                role,
                &auth.0.role,
                &auth.0.auth_source,
            ))
        })
}

fn store_err(context: &'static str) -> impl FnOnce(sqlx::Error) -> (StatusCode, Json<ApiError>) {
    move |error| {
        error!(%error, "{context}");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", context)
    }
}

#[derive(Deserialize)]
pub struct ListReleasesQuery {
    project_id: Option<String>,
    build_id: Option<String>,
    state: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

/// `GET /v1/releases` — releases the caller can see, newest first.
pub async fn list_releases(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<ListReleasesQuery>,
) -> ApiResult<ListReleasesResponse> {
    let limit = params.limit.unwrap_or(50);
    if !(1..=200).contains(&limit) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "limit must be between 1 and 200",
        ));
    }
    let offset = params.offset.unwrap_or(0);
    if offset < 0 {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "offset must not be negative",
        ));
    }

    let mut conditions = Vec::new();
    let mut binds: Vec<String> = vec![auth.0.user_id.clone()];
    if auth.0.role != "owner" && auth.0.role != "admin" {
        conditions.push("pm.user_id IS NOT NULL".to_string());
    }
    if let Some(project_id) = params.project_id {
        binds.push(project_id);
        conditions.push(format!("r.project_id = ?{}", binds.len()));
    }
    if let Some(build_id) = params.build_id {
        binds.push(build_id);
        conditions.push(format!("r.build_id = ?{}", binds.len()));
    }
    if let Some(states) = params.state {
        let mut placeholders = Vec::new();
        for value in states
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let parsed: ReleaseState = value.parse().map_err(|message: String| {
                api_err(StatusCode::BAD_REQUEST, "invalid_input", message)
            })?;
            binds.push(parsed.as_str().to_string());
            placeholders.push(format!("?{}", binds.len()));
        }
        if placeholders.is_empty() {
            conditions.push("1 = 0".to_string());
        } else {
            conditions.push(format!("r.state IN ({})", placeholders.join(", ")));
        }
    }
    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };
    let from = "FROM releases r \
         LEFT JOIN project_members pm ON pm.project_id = r.project_id AND pm.user_id = ?1";

    let mut count = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
        "SELECT COUNT(*) {from} {where_clause}"
    )));
    for value in &binds {
        count = count.bind(value);
    }
    let total = count
        .fetch_one(&state.db)
        .await
        .map_err(store_err("Failed to count releases"))?;

    let mut list = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT r.*, p.name AS project_name, b.build_number AS build_number, \
         au.email AS approved_by_email, pm.role AS member_role {from} \
         LEFT JOIN projects p ON p.id = r.project_id \
         LEFT JOIN builds b ON b.id = r.build_id \
         LEFT JOIN users au ON au.id = r.approved_by \
         {where_clause} ORDER BY r.created_at DESC, r.id LIMIT ?{} OFFSET ?{}",
        binds.len() + 1,
        binds.len() + 2
    )));
    for value in &binds {
        list = list.bind(value);
    }
    let rows = list
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .map_err(store_err("Failed to list releases"))?;

    let mut releases = Vec::with_capacity(rows.len());
    for row in &rows {
        let mut release = row_to_release(row).map_err(ReleaseError::into_api)?;
        release.allowed_actions = allowed_actions(&row_role(&auth, row), &release);
        releases.push(release);
    }
    Ok(Json(ListReleasesResponse { releases, total }))
}

fn row_to_history(row: &SqliteRow) -> ReleaseHistoryEntry {
    ReleaseHistoryEntry {
        id: row.get("id"),
        at: row.get("at"),
        actor_id: row.get("actor_id"),
        actor_email: row.get("actor_email"),
        from_state: row
            .get::<Option<String>, _>("from_state")
            .and_then(|state| state.parse().ok()),
        to_state: row
            .get::<String, _>("to_state")
            .parse()
            .unwrap_or(ReleaseState::Failed),
        from_fraction: row.get("from_fraction"),
        to_fraction: row.get("to_fraction"),
        reason: row.get("reason"),
    }
}

/// `GET /v1/releases/{release_id}` — release with its full history.
pub async fn get_release(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(release_id): Path<String>,
) -> ApiResult<ReleaseDetailResponse> {
    let mut release = load_release(&state.db, &release_id)
        .await
        .map_err(ReleaseError::into_api)?;
    let role = project_role(&state, &auth, &release.project_id).await?;
    require_project_permission(&role, ProjectPermission::Read)
        .map_err(|_| api_err(StatusCode::NOT_FOUND, "not_found", "Release not found"))?;
    release.allowed_actions = allowed_actions(&role, &release);
    let history =
        sqlx::query("SELECT * FROM release_history WHERE release_id = ?1 ORDER BY at, rowid")
            .bind(&release_id)
            .fetch_all(&state.db)
            .await
            .map_err(store_err("Failed to load release history"))?
            .iter()
            .map(row_to_history)
            .collect();
    Ok(Json(ReleaseDetailResponse { release, history }))
}

/// `POST /v1/releases` — record a release for a succeeded build.
pub async fn create_release_handler(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(request): Json<CreateReleaseRequest>,
) -> ApiResult<Release> {
    let build = sqlx::query("SELECT project_id, status FROM builds WHERE id = ?1")
        .bind(&request.build_id)
        .fetch_optional(&state.db)
        .await
        .map_err(store_err("Failed to load build"))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Build not found"))?;
    let project_id: String = build.get("project_id");
    let role = project_role(&state, &auth, &project_id).await?;
    require_project_permission(&role, ProjectPermission::ManageReleases)?;
    if build.get::<String, _>("status") != "succeeded" {
        return Err(api_err(
            StatusCode::CONFLICT,
            "build_not_succeeded",
            "Only a succeeded build can be released",
        ));
    }
    if let Some(artifact_id) = &request.artifact_id {
        let belongs: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM artifacts WHERE id = ?1 AND build_id = ?2")
                .bind(artifact_id)
                .bind(&request.build_id)
                .fetch_optional(&state.db)
                .await
                .map_err(store_err("Failed to load artifact"))?;
        if belongs.is_none() {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "artifact_id does not belong to the build",
            ));
        }
    }
    if request.version_code.is_some_and(|code| code < 0) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "version_code must not be negative",
        ));
    }
    let policy = request.policy.unwrap_or_default();
    let initial_state = request.state.unwrap_or(ReleaseState::Rolling);
    let rollout_fraction = request.rollout_fraction.unwrap_or_else(|| {
        if initial_state == ReleaseState::Rolling {
            policy.steps.first().copied().unwrap_or(0.0)
        } else {
            0.0
        }
    });
    let mut release = create_release(
        ReleaseDeps::from(&*state),
        NewRelease {
            project_id,
            build_id: request.build_id,
            artifact_id: request.artifact_id,
            store: request.store,
            track: request.track,
            package_name: request.package_name,
            version_name: request.version_name,
            version_code: request.version_code,
            state: initial_state,
            rollout_fraction,
            policy,
        },
        &user_actor(&auth),
        request.reason.as_deref(),
    )
    .await
    .map_err(ReleaseError::into_api)?;
    release.allowed_actions = allowed_actions(&role, &release);
    Ok(Json(release))
}

/// `POST /v1/releases/{release_id}/{action}` — raise, pause, resume, halt, complete or approve.
pub async fn release_action(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((release_id, action)): Path<(String, String)>,
    body: Option<Json<ReleaseActionRequest>>,
) -> ApiResult<Release> {
    let action: ReleaseAction = action
        .parse()
        .map_err(|_| api_err(StatusCode::NOT_FOUND, "not_found", "Unknown release action"))?;
    let request = body.map(|Json(request)| request).unwrap_or_default();
    let current = load_release(&state.db, &release_id)
        .await
        .map_err(ReleaseError::into_api)?;
    let role = project_role(&state, &auth, &current.project_id).await?;
    require_project_permission(&role, ProjectPermission::Read)
        .map_err(|_| api_err(StatusCode::NOT_FOUND, "not_found", "Release not found"))?;
    let permission = match action {
        ReleaseAction::Pause | ReleaseAction::Halt => ProjectPermission::OperateRelease,
        _ => ProjectPermission::ManageReleases,
    };
    require_project_permission(&role, permission)?;
    if request.fraction.is_some() && action != ReleaseAction::Raise {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "fraction is only accepted by raise",
        ));
    }
    if !allowed_actions(&role, &current).contains(&action) {
        return Err(api_err(
            StatusCode::CONFLICT,
            "invalid_release_state",
            format!(
                "Cannot {} a release that is {}",
                action.as_str(),
                current.state
            ),
        ));
    }

    let deps = ReleaseDeps::from(&*state);
    let actor = user_actor(&auth);
    let reason = request.reason.as_deref();
    let change = match action {
        ReleaseAction::Approve => {
            let mut release = approve_release(deps, &release_id, &actor, reason)
                .await
                .map_err(ReleaseError::into_api)?;
            release.allowed_actions = allowed_actions(&role, &release);
            return Ok(Json(release));
        }
        ReleaseAction::Raise => {
            let fraction = match request.fraction {
                Some(fraction) => fraction,
                None => current
                    .policy
                    .next_step(current.rollout_fraction)
                    .unwrap_or(1.0),
            };
            if !fraction.is_finite() || fraction <= current.rollout_fraction || fraction > 1.0 {
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_input",
                    "fraction must be above the current rollout and at most 1",
                ));
            }
            ReleaseChange {
                to_state: if fraction >= 1.0 {
                    ReleaseState::Complete
                } else {
                    ReleaseState::Rolling
                },
                to_fraction: Some(fraction),
                audit_action: "release_raised",
            }
        }
        ReleaseAction::Pause => ReleaseChange {
            to_state: ReleaseState::Paused,
            to_fraction: None,
            audit_action: "release_paused",
        },
        // A draft (0%) resumes at the first policy step.
        ReleaseAction::Resume => ReleaseChange {
            to_state: ReleaseState::Rolling,
            to_fraction: (current.rollout_fraction == 0.0)
                .then(|| current.policy.steps.first().copied())
                .flatten(),
            audit_action: "release_resumed",
        },
        ReleaseAction::Halt => ReleaseChange {
            to_state: ReleaseState::Halted,
            to_fraction: None,
            audit_action: "release_halted",
        },
        ReleaseAction::Complete => ReleaseChange {
            to_state: ReleaseState::Complete,
            to_fraction: Some(1.0),
            audit_action: "release_completed",
        },
    };
    let mut release = transition_release(deps, &release_id, change, &actor, reason)
        .await
        .map_err(ReleaseError::into_api)?;
    release.allowed_actions = allowed_actions(&role, &release);
    Ok(Json(release))
}

#[derive(Deserialize)]
pub struct ReleaseActivityQuery {
    project_id: Option<String>,
    limit: Option<i64>,
}

/// `GET /v1/release-events` — recent release history across visible projects.
pub async fn list_release_activity(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<ReleaseActivityQuery>,
) -> ApiResult<ListReleaseActivityResponse> {
    let limit = params.limit.unwrap_or(50);
    if !(1..=200).contains(&limit) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "limit must be between 1 and 200",
        ));
    }
    let admin = auth.0.role == "owner" || auth.0.role == "admin";
    let rows = sqlx::query(
        "SELECT h.*, r.project_id, r.build_id, r.store, r.track, r.version_name, p.name AS project_name \
         FROM release_history h \
         JOIN releases r ON r.id = h.release_id \
         LEFT JOIN projects p ON p.id = r.project_id \
         WHERE (?1 = 1 OR EXISTS (SELECT 1 FROM project_members pm \
                WHERE pm.project_id = r.project_id AND pm.user_id = ?2)) \
           AND (?3 IS NULL OR r.project_id = ?3) \
         ORDER BY h.at DESC, h.rowid DESC LIMIT ?4",
    )
    .bind(admin)
    .bind(&auth.0.user_id)
    .bind(params.project_id)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(store_err("Failed to load release activity"))?;

    let events = rows
        .iter()
        .filter_map(|row| {
            let store = row.get::<String, _>("store").parse().ok()?;
            Some(ReleaseActivityEvent {
                entry: row_to_history(row),
                release_id: row.get("release_id"),
                project_id: row.get("project_id"),
                project_name: row.get("project_name"),
                build_id: row.get("build_id"),
                store,
                track: row.get("track"),
                version_name: row.get("version_name"),
            })
        })
        .collect();
    Ok(Json(ListReleaseActivityResponse { events }))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct FakeStore {
        changes: Mutex<Vec<StoreChange>>,
        crash_rate: Mutex<Option<f64>>,
    }

    impl ReleaseStoreClient for FakeStore {
        fn apply<'a>(
            &'a self,
            _: &'a Release,
            change: StoreChange,
        ) -> BoxFuture<'a, anyhow::Result<()>> {
            self.changes.lock().unwrap().push(change);
            Box::pin(async { Ok(()) })
        }

        fn crash_rate<'a>(&'a self, _: &'a Release) -> BoxFuture<'a, anyhow::Result<Option<f64>>> {
            let rate = *self.crash_rate.lock().unwrap();
            Box::pin(async move { Ok(rate) })
        }
    }

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
            "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, queued_at, created_at, updated_at) \
             VALUES ('b1', 'p1', 'pl1', 7, 'succeeded', 'manual', 0, 0, 0)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        pool
    }

    fn new_release(track: &str, policy: ReleasePolicy) -> NewRelease {
        NewRelease {
            project_id: "p1".into(),
            build_id: "b1".into(),
            artifact_id: None,
            store: ReleaseStore::GooglePlay,
            track: track.into(),
            package_name: Some("com.example.app".into()),
            version_name: Some("1.2.0".into()),
            version_code: Some(120),
            state: ReleaseState::Rolling,
            rollout_fraction: 0.01,
            policy,
        }
    }

    fn user() -> ReleaseActor {
        ReleaseActor::User {
            user_id: "u1".into(),
            email: "dev@example.com".into(),
        }
    }

    fn change(to_state: ReleaseState, audit_action: &'static str) -> ReleaseChange {
        ReleaseChange {
            to_state,
            to_fraction: None,
            audit_action,
        }
    }

    async fn history_len(pool: &SqlitePool, release_id: &str) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM release_history WHERE release_id = ?1")
            .bind(release_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn daemon_walks_the_default_schedule_to_complete() {
        let pool = pool().await;
        let scheduler = Scheduler::new(16);
        let store = FakeStore::default();
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let mut events = scheduler.subscribe_release_events();

        let release = create_release(
            deps,
            new_release("production", ReleasePolicy::default()),
            &user(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(events.recv().await.unwrap().kind, ReleaseEventKind::Created);
        let mut at = release
            .next_step_at
            .expect("rolling play release is scheduled");

        run_rollout_tick(deps, at - 1, 100).await.unwrap();
        assert_eq!(
            load_release(&pool, &release.id)
                .await
                .unwrap()
                .rollout_fraction,
            0.01
        );

        for expected in [0.05, 0.2, 0.5] {
            run_rollout_tick(deps, at, 100).await.unwrap();
            let current = load_release(&pool, &release.id).await.unwrap();
            assert_eq!(current.rollout_fraction, expected);
            assert_eq!(current.state, ReleaseState::Rolling);
            at = current.next_step_at.unwrap();
        }
        run_rollout_tick(deps, at, 100).await.unwrap();
        let done = load_release(&pool, &release.id).await.unwrap();
        assert_eq!(done.state, ReleaseState::Complete);
        assert_eq!(done.rollout_fraction, 1.0);
        assert_eq!(done.next_step_at, None);
        assert_eq!(
            *store.changes.lock().unwrap(),
            vec![
                StoreChange::Rollout { fraction: 0.05 },
                StoreChange::Rollout { fraction: 0.2 },
                StoreChange::Rollout { fraction: 0.5 },
                StoreChange::Complete,
            ]
        );
        assert_eq!(history_len(&pool, &release.id).await, 5);
        let audits: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE resource_type = 'release' AND resource_id = ?1 \
             AND action = 'release_advanced' AND actor_id IS NULL",
        )
        .bind(&release.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(audits, 4);
    }

    #[tokio::test]
    async fn crash_rate_breach_halts_and_halted_never_advances_until_resumed() {
        let pool = pool().await;
        let scheduler = Scheduler::new(16);
        let store = FakeStore::default();
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let policy = ReleasePolicy {
            crash_rate_threshold: Some(0.02),
            ..ReleasePolicy::default()
        };
        let release = create_release(deps, new_release("production", policy), &user(), None)
            .await
            .unwrap();
        let due = release.next_step_at.unwrap();

        *store.crash_rate.lock().unwrap() = Some(0.03);
        run_rollout_tick(deps, due, 100).await.unwrap();
        let halted = load_release(&pool, &release.id).await.unwrap();
        assert_eq!(halted.state, ReleaseState::Halted);
        assert_eq!(halted.rollout_fraction, 0.01);
        assert_eq!(halted.health, ReleaseHealth::Breached);
        assert_eq!(halted.next_step_at, None);

        *store.crash_rate.lock().unwrap() = Some(0.0);
        for later in [due + 1, due + 10 * 86_400] {
            run_rollout_tick(deps, later, 100).await.unwrap();
            let still = load_release(&pool, &release.id).await.unwrap();
            assert_eq!(still.state, ReleaseState::Halted);
            assert_eq!(still.rollout_fraction, 0.01);
        }
        assert_eq!(*store.changes.lock().unwrap(), vec![StoreChange::Halt]);

        let resumed = transition_release(
            deps,
            &release.id,
            change(ReleaseState::Rolling, "release_resumed"),
            &user(),
            Some("fixed"),
        )
        .await
        .unwrap();
        assert_eq!(resumed.state, ReleaseState::Rolling);
        assert!(resumed.next_step_at.is_some());
        let reason: Option<String> = sqlx::query_scalar(
            "SELECT reason FROM release_history WHERE release_id = ?1 AND to_state = 'halted'",
        )
        .bind(&release.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(reason.unwrap().contains("crash rate"));
    }

    #[tokio::test]
    async fn manual_changes_record_actor_and_reject_invalid_moves() {
        let pool = pool().await;
        let scheduler = Scheduler::new(16);
        let store = FakeStore::default();
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let release = create_release(
            deps,
            new_release("beta", ReleasePolicy::default()),
            &user(),
            None,
        )
        .await
        .unwrap();

        let paused = transition_release(
            deps,
            &release.id,
            change(ReleaseState::Paused, "release_paused"),
            &user(),
            Some("  checking logs  "),
        )
        .await
        .unwrap();
        assert_eq!(paused.next_step_at, None);
        let row = sqlx::query(
            "SELECT actor_id, actor_email, reason FROM release_history WHERE release_id = ?1 AND to_state = 'paused'",
        )
        .bind(&release.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            row.get::<Option<String>, _>("actor_id").as_deref(),
            Some("u1")
        );
        assert_eq!(
            row.get::<Option<String>, _>("actor_email").as_deref(),
            Some("dev@example.com")
        );
        assert_eq!(
            row.get::<Option<String>, _>("reason").as_deref(),
            Some("checking logs")
        );
        let audit: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE action = 'release_paused' AND actor_id = 'u1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(audit, 1);

        let raise_paused = transition_release(
            deps,
            &release.id,
            ReleaseChange {
                to_state: ReleaseState::Paused,
                to_fraction: Some(0.5),
                audit_action: "release_raised",
            },
            &user(),
            None,
        )
        .await;
        assert!(matches!(raise_paused, Err(ReleaseError::Conflict(_))));

        transition_release(
            deps,
            &release.id,
            change(ReleaseState::Halted, "release_halted"),
            &user(),
            None,
        )
        .await
        .unwrap();
        let complete_halted = transition_release(
            deps,
            &release.id,
            change(ReleaseState::Complete, "release_completed"),
            &user(),
            None,
        )
        .await;
        assert!(matches!(complete_halted, Err(ReleaseError::Conflict(_))));

        transition_release(
            deps,
            &release.id,
            change(ReleaseState::Rolling, "release_resumed"),
            &user(),
            None,
        )
        .await
        .unwrap();
        let lower = transition_release(
            deps,
            &release.id,
            ReleaseChange {
                to_state: ReleaseState::Rolling,
                to_fraction: Some(0.005),
                audit_action: "release_raised",
            },
            &user(),
            None,
        )
        .await;
        assert!(matches!(lower, Err(ReleaseError::Invalid(_))));
        let complete = transition_release(
            deps,
            &release.id,
            change(ReleaseState::Complete, "release_completed"),
            &user(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(complete.rollout_fraction, 1.0);
        assert!(complete.allowed_actions.is_empty());
    }

    #[tokio::test]
    async fn production_gate_blocks_rollout_until_approved() {
        let pool = pool().await;
        let scheduler = Scheduler::new(16);
        let store = FakeStore::default();
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let mut events = scheduler.subscribe_release_events();
        let policy = ReleasePolicy {
            approval: ReleaseApprovalGate::BeforeProduction,
            ..ReleasePolicy::default()
        };
        let mut input = new_release("production", policy);
        input.rollout_fraction = 0.0;
        let release = create_release(deps, input, &user(), None).await.unwrap();
        assert!(release.awaiting_approval);
        assert_eq!(release.next_step_at, None);
        assert_eq!(events.recv().await.unwrap().kind, ReleaseEventKind::Created);
        assert_eq!(
            events.recv().await.unwrap().kind,
            ReleaseEventKind::Uploaded
        );
        assert_eq!(
            events.recv().await.unwrap().kind,
            ReleaseEventKind::ApprovalNeeded
        );

        let maintainer = EffectiveProjectRole::Member(ProjectRole::Maintainer);
        assert_eq!(
            allowed_actions(&maintainer, &release),
            vec![
                ReleaseAction::Pause,
                ReleaseAction::Halt,
                ReleaseAction::Approve
            ]
        );
        let blocked = transition_release(
            deps,
            &release.id,
            ReleaseChange {
                to_state: ReleaseState::Rolling,
                to_fraction: Some(0.05),
                audit_action: "release_raised",
            },
            &user(),
            None,
        )
        .await;
        assert!(matches!(blocked, Err(ReleaseError::Conflict(_))));
        run_rollout_tick(deps, now_unix() + 365 * 86_400, 100)
            .await
            .unwrap();
        assert_eq!(
            load_release(&pool, &release.id)
                .await
                .unwrap()
                .rollout_fraction,
            0.0
        );

        let approved = approve_release(deps, &release.id, &user(), None)
            .await
            .unwrap();
        assert!(!approved.awaiting_approval);
        assert_eq!(
            approved.approved_by_email.as_deref(),
            Some("dev@example.com")
        );
        run_rollout_tick(deps, approved.next_step_at.unwrap(), 100)
            .await
            .unwrap();
        assert_eq!(
            load_release(&pool, &release.id)
                .await
                .unwrap()
                .rollout_fraction,
            0.01
        );
    }

    #[tokio::test]
    async fn app_store_releases_are_not_driven_by_the_daemon() {
        let pool = pool().await;
        let scheduler = Scheduler::new(16);
        let store = FakeStore::default();
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let mut input = new_release("app_store", ReleasePolicy::default());
        input.store = ReleaseStore::AppStore;
        let release = create_release(deps, input, &ReleaseActor::System, None)
            .await
            .unwrap();
        assert_eq!(release.platform, "ios");
        assert_eq!(release.next_step_at, None);
        run_rollout_tick(deps, now_unix() + 365 * 86_400, 100)
            .await
            .unwrap();
        assert_eq!(
            load_release(&pool, &release.id)
                .await
                .unwrap()
                .rollout_fraction,
            0.01
        );
        assert!(store.changes.lock().unwrap().is_empty());
    }

    #[test]
    fn actions_follow_project_role() {
        let release = Release {
            id: "r".into(),
            project_id: "p".into(),
            project_name: None,
            build_id: "b".into(),
            build_number: None,
            artifact_id: None,
            platform: "android".into(),
            store: ReleaseStore::GooglePlay,
            track: "production".into(),
            package_name: None,
            version_name: None,
            version_code: None,
            state: ReleaseState::Rolling,
            rollout_fraction: 0.2,
            next_fraction: Some(0.5),
            next_step_at: None,
            policy: ReleasePolicy::default(),
            health: ReleaseHealth::Unknown,
            crash_rate: None,
            crash_rate_threshold: None,
            awaiting_approval: false,
            approved_at: None,
            approved_by_email: None,
            allowed_actions: Vec::new(),
            created_at: 0,
            updated_at: 0,
        };
        use ReleaseAction::{Complete, Halt, Pause, Raise, Resume};
        let viewer = EffectiveProjectRole::Member(ProjectRole::Viewer);
        let developer = EffectiveProjectRole::Member(ProjectRole::Developer);
        assert!(allowed_actions(&viewer, &release).is_empty());
        assert!(allowed_actions(&EffectiveProjectRole::None, &release).is_empty());
        assert_eq!(allowed_actions(&developer, &release), vec![Pause, Halt]);
        assert_eq!(
            allowed_actions(&EffectiveProjectRole::InstanceAdmin, &release),
            vec![Raise, Pause, Halt, Complete]
        );
        let halted = Release {
            state: ReleaseState::Halted,
            ..release
        };
        assert!(allowed_actions(&developer, &halted).is_empty());
        assert_eq!(
            allowed_actions(&EffectiveProjectRole::InstanceAdmin, &halted),
            vec![Resume]
        );
    }

    #[test]
    fn policy_validation_rejects_bad_schedules() {
        assert!(ReleasePolicy::default().validate().is_ok());
        for steps in [
            vec![],
            vec![0.5, 0.2, 1.0],
            vec![0.1, 0.5],
            vec![0.0, 1.0],
            vec![0.5, 1.5],
        ] {
            let policy = ReleasePolicy {
                steps,
                ..ReleasePolicy::default()
            };
            assert!(policy.validate().is_err());
        }
        let policy = ReleasePolicy {
            interval_hours: 0.0,
            ..ReleasePolicy::default()
        };
        assert!(policy.validate().is_err());
    }
}
