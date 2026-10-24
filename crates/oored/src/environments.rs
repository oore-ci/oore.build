//! Project environments and promotion of a build between them.
//!
//! An environment names a store target (store and track), the rollout policy
//! for releases promoted into it, and whether a promotion needs a
//! maintainer's approval. Promoting a build puts the version an earlier
//! release already uploaded on the environment's track, then records a new
//! release with the environment's policy, so the rollout monitor drives it
//! like any other release. Nothing is rebuilt or uploaded again.
//!
//! Projects without environments are unaffected: releases keep a NULL
//! `environment_id` and the per-pipeline publish settings apply as before.
//! A release recorded on a track that an environment targets is placed in
//! that environment, so the board shows uploads too.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, Weak};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use oore_contract::{
    ApiError, CreateProjectEnvironmentRequest, CreatePromotionRequest, EnvironmentPromotion,
    ListProjectEnvironmentsResponse, OkResponse, ProjectEnvironment, PromotionDecisionRequest,
    PromotionState, Release, ReleaseApprovalGate, ReleasePolicy, ReleaseState, ReleaseStore,
    UpdateProjectEnvironmentRequest,
};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::project_rbac::{
    EffectiveProjectRole, ProjectPermission, check_project_permission, require_project_permission,
};
use crate::releases::{
    NewRelease, PromotionTarget, RELEASE_SELECT, ReleaseActor, ReleaseDeps, ReleaseError,
    allowed_actions, finish_create, insert_release, project_role, row_to_release, user_actor,
};
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

/// Environments one project may define.
const MAX_ENVIRONMENTS: i64 = 12;
const MAX_REASON_LEN: usize = 500;
const MAX_NAME_LEN: usize = 64;
const MAX_KEY_LEN: usize = 32;
/// Decided promotions listed with the environments, after every pending one.
const RECENT_PROMOTIONS: i64 = 20;

// ── Errors ──────────────────────────────────────────────────────

#[derive(Debug)]
pub enum EnvironmentError {
    NotFound(&'static str),
    Invalid(String),
    Forbidden(String),
    Conflict { code: &'static str, message: String },
    Store(anyhow::Error),
    Db(anyhow::Error),
}

impl From<sqlx::Error> for EnvironmentError {
    fn from(error: sqlx::Error) -> Self {
        Self::Db(error.into())
    }
}

impl From<ReleaseError> for EnvironmentError {
    fn from(error: ReleaseError) -> Self {
        match error {
            ReleaseError::NotFound => Self::NotFound("Release not found"),
            ReleaseError::Invalid(message) => Self::Invalid(message),
            ReleaseError::Conflict(message) => Self::Conflict {
                code: "invalid_release_state",
                message,
            },
            ReleaseError::Store(error) => Self::Store(error),
            ReleaseError::Db(error) => Self::Db(error),
        }
    }
}

impl EnvironmentError {
    fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self::Conflict {
            code,
            message: message.into(),
        }
    }

    pub fn into_api(self) -> (StatusCode, Json<ApiError>) {
        match self {
            Self::NotFound(message) => api_err(StatusCode::NOT_FOUND, "not_found", message),
            Self::Invalid(message) => api_err(StatusCode::BAD_REQUEST, "invalid_input", message),
            Self::Forbidden(message) => {
                api_err(StatusCode::FORBIDDEN, "permission_denied", message)
            }
            Self::Conflict { code, message } => api_err(StatusCode::CONFLICT, code, message),
            Self::Store(error) => {
                warn!(error = %error, "store rejected promotion");
                api_err(
                    StatusCode::BAD_GATEWAY,
                    "store_rejected",
                    format!("The store did not accept the promotion: {}", short(&error)),
                )
            }
            Self::Db(error) => {
                error!(error = %error, "environment store error");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to update environments",
                )
            }
        }
    }
}

/// One line of an error, short enough for a reason column.
fn short(error: &anyhow::Error) -> String {
    format!("{error:#}")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_REASON_LEN)
        .collect()
}

fn clean_reason(reason: Option<&str>) -> Option<String> {
    reason
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
        .map(|reason| reason.chars().take(MAX_REASON_LEN).collect())
}

// ── Validation ──────────────────────────────────────────────────

fn validate_key(key: &str) -> Result<String, EnvironmentError> {
    let key = key.trim();
    let valid = !key.is_empty()
        && key.len() <= MAX_KEY_LEN
        && key.starts_with(|c: char| c.is_ascii_lowercase())
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if valid {
        Ok(key.to_string())
    } else {
        Err(EnvironmentError::Invalid(format!(
            "key must be 1 to {MAX_KEY_LEN} lowercase letters, digits or dashes, starting with a letter"
        )))
    }
}

fn validate_name(name: &str) -> Result<String, EnvironmentError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_LEN {
        return Err(EnvironmentError::Invalid(format!(
            "name must be 1 to {MAX_NAME_LEN} characters"
        )));
    }
    Ok(name.to_string())
}

fn validate_track(store: ReleaseStore, track: &str) -> Result<String, EnvironmentError> {
    let track = track.trim();
    let valid = match store {
        ReleaseStore::GooglePlay => {
            !track.is_empty()
                && track.len() <= 128
                && track
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':'))
        }
        ReleaseStore::AppStore => !track.is_empty() && track.len() <= 256,
    };
    if valid {
        Ok(track.to_string())
    } else {
        Err(EnvironmentError::Invalid(match store {
            ReleaseStore::GooglePlay => {
                "track must be internal, closed, open, production, or a custom track name".into()
            }
            ReleaseStore::AppStore => {
                "track must name the App Store track, such as app_store or a TestFlight group"
                    .into()
            }
        }))
    }
}

fn validate_policy(policy: &ReleasePolicy) -> Result<(), EnvironmentError> {
    policy.validate().map_err(|message| {
        EnvironmentError::Invalid(message.replace("policy.", "rollout_policy."))
    })?;
    if policy.approval != ReleaseApprovalGate::None {
        return Err(EnvironmentError::Invalid(
            "rollout_policy.approval must be none; use approval_required on the environment".into(),
        ));
    }
    Ok(())
}

// ── Rows ────────────────────────────────────────────────────────

fn row_to_environment(row: &SqliteRow) -> Result<ProjectEnvironment, EnvironmentError> {
    let store: ReleaseStore = row
        .get::<String, _>("store")
        .parse()
        .map_err(|message: String| EnvironmentError::Db(anyhow::anyhow!(message)))?;
    let rollout_policy: ReleasePolicy = serde_json::from_str(&row.get::<String, _>("policy_json"))
        .map_err(|error| EnvironmentError::Db(error.into()))?;
    Ok(ProjectEnvironment {
        id: row.get("id"),
        project_id: row.get("project_id"),
        key: row.get("env_key"),
        name: row.get("name"),
        position: row.get("position"),
        store,
        track: row.get("track"),
        approval_required: row.get::<i64, _>("approval_required") != 0,
        rollout_policy,
        latest_release: None,
        promotable: store == ReleaseStore::GooglePlay,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

pub(crate) async fn load_environment(
    pool: &SqlitePool,
    project_id: &str,
    environment_id: &str,
) -> Result<ProjectEnvironment, EnvironmentError> {
    let row = sqlx::query("SELECT * FROM project_environments WHERE id = ?1 AND project_id = ?2")
        .bind(environment_id)
        .bind(project_id)
        .fetch_optional(pool)
        .await?
        .ok_or(EnvironmentError::NotFound("Environment not found"))?;
    row_to_environment(&row)
}

const PROMOTION_SELECT: &str = "SELECT pr.*, e.name AS environment_name, b.build_number AS build_number, \
     sr.version_name AS version_name, sr.version_code AS version_code, sr.track AS source_track, \
     se.name AS source_environment_name, ru.email AS requested_by_email, du.email AS decided_by_email \
     FROM environment_promotions pr \
     JOIN project_environments e ON e.id = pr.environment_id \
     JOIN releases sr ON sr.id = pr.source_release_id \
     LEFT JOIN project_environments se ON se.id = sr.environment_id \
     LEFT JOIN builds b ON b.id = pr.build_id \
     LEFT JOIN users ru ON ru.id = pr.requested_by \
     LEFT JOIN users du ON du.id = pr.decided_by";

fn row_to_promotion(
    row: &SqliteRow,
    role: &EffectiveProjectRole,
) -> Result<EnvironmentPromotion, EnvironmentError> {
    let state: PromotionState = row
        .get::<String, _>("state")
        .parse()
        .map_err(|message: String| EnvironmentError::Db(anyhow::anyhow!(message)))?;
    Ok(EnvironmentPromotion {
        id: row.get("id"),
        project_id: row.get("project_id"),
        build_id: row.get("build_id"),
        build_number: row.get("build_number"),
        version_name: row.get("version_name"),
        version_code: row.get("version_code"),
        environment_id: row.get("environment_id"),
        environment_name: row.get("environment_name"),
        source_release_id: row.get("source_release_id"),
        source_environment_name: row.get("source_environment_name"),
        source_track: row.get("source_track"),
        release_id: row.get("release_id"),
        state,
        reason: row.get("reason"),
        decision_reason: row.get("decision_reason"),
        error: row.get("error"),
        requested_by_email: row.get("requested_by_email"),
        decided_by_email: row.get("decided_by_email"),
        decided_at: row.get("decided_at"),
        can_decide: state == PromotionState::Pending
            && check_project_permission(role, ProjectPermission::ManageReleases),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

async fn load_promotion(
    pool: &SqlitePool,
    project_id: &str,
    promotion_id: &str,
    role: &EffectiveProjectRole,
) -> Result<EnvironmentPromotion, EnvironmentError> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{PROMOTION_SELECT} WHERE pr.id = ?1 AND pr.project_id = ?2"
    )))
    .bind(promotion_id)
    .bind(project_id)
    .fetch_optional(pool)
    .await?
    .ok_or(EnvironmentError::NotFound("Promotion not found"))?;
    row_to_promotion(&row, role)
}

async fn audit(
    pool: &SqlitePool,
    actor: &ReleaseActor,
    action: &str,
    resource_type: &str,
    resource_id: &str,
    details: serde_json::Value,
) {
    let actor_id = match actor {
        ReleaseActor::User { user_id, .. } => Some(user_id.as_str()),
        ReleaseActor::System => None,
    };
    if let Err(error) = write_audit_log(
        pool,
        actor_id,
        action,
        resource_type,
        Some(resource_id),
        Some(&details.to_string()),
    )
    .await
    {
        warn!(resource_id, error = %error, "failed to write environment audit log");
    }
}

fn actor_email(actor: &ReleaseActor) -> Option<&str> {
    match actor {
        ReleaseActor::User { email, .. } => Some(email),
        ReleaseActor::System => None,
    }
}

fn actor_id(actor: &ReleaseActor) -> Option<&str> {
    match actor {
        ReleaseActor::User { user_id, .. } => Some(user_id),
        ReleaseActor::System => None,
    }
}

fn unique_violation(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(db) if db.is_unique_violation())
}

// ── Environments ────────────────────────────────────────────────

pub(crate) async fn create_environment(
    pool: &SqlitePool,
    project_id: &str,
    actor: &ReleaseActor,
    request: CreateProjectEnvironmentRequest,
) -> Result<ProjectEnvironment, EnvironmentError> {
    let key = validate_key(&request.key)?;
    let name = validate_name(&request.name)?;
    let track = validate_track(request.store, &request.track)?;
    let policy = request.rollout_policy.unwrap_or_default();
    validate_policy(&policy)?;
    let policy_json =
        serde_json::to_string(&policy).map_err(|error| EnvironmentError::Db(error.into()))?;

    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let (count, next_position): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(MAX(position) + 1, 0) FROM project_environments WHERE project_id = ?1",
    )
    .bind(project_id)
    .fetch_one(&mut *tx)
    .await?;
    if count >= MAX_ENVIRONMENTS {
        return Err(EnvironmentError::Invalid(format!(
            "a project can have at most {MAX_ENVIRONMENTS} environments"
        )));
    }
    let id = Uuid::new_v4().to_string();
    let now = now_unix();
    let inserted = sqlx::query(
        "INSERT INTO project_environments (id, project_id, env_key, name, position, store, track, \
         approval_required, policy_json, created_by, updated_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10, ?11, ?11)",
    )
    .bind(&id)
    .bind(project_id)
    .bind(&key)
    .bind(&name)
    .bind(next_position)
    .bind(request.store.as_str())
    .bind(&track)
    .bind(request.approval_required)
    .bind(&policy_json)
    .bind(actor_id(actor))
    .bind(now)
    .execute(&mut *tx)
    .await;
    if let Err(error) = inserted {
        return Err(if unique_violation(&error) {
            EnvironmentError::conflict(
                "environment_exists",
                "Another environment already uses this key, or this store and track",
            )
        } else {
            error.into()
        });
    }
    tx.commit().await?;

    let environment = load_environment(pool, project_id, &id).await?;
    audit(
        pool,
        actor,
        "environment_created",
        "project_environment",
        &id,
        serde_json::json!({
            "actor_email": actor_email(actor),
            "project_id": project_id,
            "key": environment.key,
            "store": environment.store,
            "track": environment.track,
            "approval_required": environment.approval_required,
            "rollout_policy": environment.rollout_policy,
        }),
    )
    .await;
    info!(project_id, environment = %environment.key, "environment created");
    Ok(environment)
}

pub(crate) async fn update_environment(
    pool: &SqlitePool,
    project_id: &str,
    environment_id: &str,
    actor: &ReleaseActor,
    request: UpdateProjectEnvironmentRequest,
) -> Result<ProjectEnvironment, EnvironmentError> {
    let current = load_environment(pool, project_id, environment_id).await?;
    let name = match &request.name {
        Some(name) => validate_name(name)?,
        None => current.name.clone(),
    };
    let track = match &request.track {
        Some(track) => validate_track(current.store, track)?,
        None => current.track.clone(),
    };
    if track != current.track {
        let has_releases: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM releases WHERE environment_id = ?1 LIMIT 1")
                .bind(environment_id)
                .fetch_optional(pool)
                .await?;
        if has_releases.is_some() {
            return Err(EnvironmentError::Invalid(format!(
                "{} already has releases on the {} track, so its track cannot change. \
                 Add a new environment for the {} track instead.",
                current.name, current.track, track
            )));
        }
    }
    let policy = request
        .rollout_policy
        .clone()
        .unwrap_or_else(|| current.rollout_policy.clone());
    validate_policy(&policy)?;
    let approval_required = request
        .approval_required
        .unwrap_or(current.approval_required);
    let position = request.position.unwrap_or(current.position);
    if position < 0 {
        return Err(EnvironmentError::Invalid(
            "position must not be negative".into(),
        ));
    }
    let policy_json =
        serde_json::to_string(&policy).map_err(|error| EnvironmentError::Db(error.into()))?;
    let updated = sqlx::query(
        "UPDATE project_environments SET name = ?1, track = ?2, approval_required = ?3, \
         policy_json = ?4, position = ?5, updated_by = ?6, updated_at = ?7 \
         WHERE id = ?8 AND project_id = ?9",
    )
    .bind(&name)
    .bind(&track)
    .bind(approval_required)
    .bind(&policy_json)
    .bind(position)
    .bind(actor_id(actor))
    .bind(now_unix())
    .bind(environment_id)
    .bind(project_id)
    .execute(pool)
    .await;
    match updated {
        Err(error) if unique_violation(&error) => {
            return Err(EnvironmentError::conflict(
                "environment_exists",
                "Another environment already uses this store and track",
            ));
        }
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let environment = load_environment(pool, project_id, environment_id).await?;
    audit(
        pool,
        actor,
        "environment_updated",
        "project_environment",
        environment_id,
        serde_json::json!({
            "actor_email": actor_email(actor),
            "project_id": project_id,
            "key": environment.key,
            "before": {
                "name": current.name,
                "track": current.track,
                "approval_required": current.approval_required,
                "rollout_policy": current.rollout_policy,
                "position": current.position,
            },
            "after": {
                "name": environment.name,
                "track": environment.track,
                "approval_required": environment.approval_required,
                "rollout_policy": environment.rollout_policy,
                "position": environment.position,
            },
        }),
    )
    .await;
    Ok(environment)
}

/// Delete an environment. Its releases stay and lose the environment.
///
/// Pending promotions are rejected and audited first, and `promoting` rows
/// that no call is working on (left by a call that never finished) are marked
/// failed. A promotion whose store call is running right now still blocks the
/// delete. The promotion rows then go with the environment.
pub(crate) async fn delete_environment(
    pool: &SqlitePool,
    project_id: &str,
    environment_id: &str,
    actor: &ReleaseActor,
) -> Result<(), EnvironmentError> {
    let current = load_environment(pool, project_id, environment_id).await?;
    let open: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, build_id, state FROM environment_promotions \
         WHERE environment_id = ?1 AND state IN ('pending', 'promoting')",
    )
    .bind(environment_id)
    .fetch_all(pool)
    .await?;
    // Hold every open promotion's lock until the row is gone, so no request,
    // approval or store call starts on it in between.
    let mut guards = Vec::with_capacity(open.len());
    for (id, build_id, state) in &open {
        let lock = promotion_lock(build_id, environment_id);
        let Ok(guard) = lock.try_lock_owned() else {
            return Err(EnvironmentError::conflict(
                "promotion_in_progress",
                "A promotion into this environment is in progress; try again when it finishes",
            ));
        };
        guards.push(guard);
        let still: Option<String> =
            sqlx::query_scalar("SELECT state FROM environment_promotions WHERE id = ?1")
                .bind(id)
                .fetch_optional(pool)
                .await?;
        match still.as_deref() {
            Some("pending") => {
                sqlx::query(
                    "UPDATE environment_promotions SET state = 'rejected', decided_by = ?1, \
                     decided_at = ?2, decision_reason = ?3, updated_at = ?2 \
                     WHERE id = ?4 AND state = 'pending'",
                )
                .bind(actor_id(actor))
                .bind(now_unix())
                .bind(ENVIRONMENT_DELETED)
                .bind(id)
                .execute(pool)
                .await?;
                audit(
                    pool,
                    actor,
                    "promotion_rejected",
                    "environment_promotion",
                    id,
                    serde_json::json!({
                        "actor_email": actor_email(actor),
                        "project_id": project_id,
                        "build_id": build_id,
                        "environment": current.key,
                        "reason": ENVIRONMENT_DELETED,
                    }),
                )
                .await;
            }
            Some("promoting") => {
                mark_failed(pool, id, STALE_PROMOTING).await?;
                audit(
                    pool,
                    actor,
                    "promotion_failed",
                    "environment_promotion",
                    id,
                    serde_json::json!({
                        "actor_email": actor_email(actor),
                        "project_id": project_id,
                        "build_id": build_id,
                        "environment": current.key,
                        "error": STALE_PROMOTING,
                        "previous_state": state,
                    }),
                )
                .await;
            }
            _ => {}
        }
    }
    sqlx::query("DELETE FROM project_environments WHERE id = ?1 AND project_id = ?2")
        .bind(environment_id)
        .bind(project_id)
        .execute(pool)
        .await?;
    audit(
        pool,
        actor,
        "environment_deleted",
        "project_environment",
        environment_id,
        serde_json::json!({
            "actor_email": actor_email(actor),
            "project_id": project_id,
            "key": current.key,
            "store": current.store,
            "track": current.track,
        }),
    )
    .await;
    Ok(())
}

pub(crate) async fn list_environments(
    pool: &SqlitePool,
    project_id: &str,
    role: &EffectiveProjectRole,
) -> Result<ListProjectEnvironmentsResponse, EnvironmentError> {
    let rows = sqlx::query(
        "SELECT * FROM project_environments WHERE project_id = ?1 ORDER BY position, created_at",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    let mut environments = Vec::with_capacity(rows.len());
    for row in &rows {
        let mut environment = row_to_environment(row)?;
        let latest = sqlx::query(sqlx::AssertSqlSafe(format!(
            "{RELEASE_SELECT} WHERE r.environment_id = ?1 ORDER BY r.created_at DESC, r.id LIMIT 1"
        )))
        .bind(&environment.id)
        .fetch_optional(pool)
        .await?;
        if let Some(latest) = latest {
            let mut release = row_to_release(&latest)?;
            release.allowed_actions = allowed_actions(role, &release);
            environment.latest_release = Some(release);
        }
        environments.push(environment);
    }

    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT * FROM ({PROMOTION_SELECT} WHERE pr.project_id = ?1 \
           AND pr.state IN ('pending', 'promoting')) \
         UNION ALL SELECT * FROM ({PROMOTION_SELECT} WHERE pr.project_id = ?1 \
           AND pr.state NOT IN ('pending', 'promoting') \
           ORDER BY pr.created_at DESC LIMIT ?2) \
         ORDER BY created_at DESC, id"
    )))
    .bind(project_id)
    .bind(RECENT_PROMOTIONS)
    .fetch_all(pool)
    .await?;
    let promotions = rows
        .iter()
        .map(|row| row_to_promotion(row, role))
        .collect::<Result<_, _>>()?;
    Ok(ListProjectEnvironmentsResponse {
        environments,
        promotions,
    })
}

// ── Promotion ───────────────────────────────────────────────────

/// One lock per build and target environment, held from the checks through
/// the store call to the recorded outcome, so a build is never promoted into
/// one environment twice at once.
fn promotion_lock(build_id: &str, environment_id: &str) -> Arc<tokio::sync::Mutex<()>> {
    static LOCKS: LazyLock<Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>> =
        LazyLock::new(Mutex::default);
    let key = format!("{build_id}\n{environment_id}");
    let mut locks = LOCKS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(lock) = locks.get(&key).and_then(Weak::upgrade) {
        return lock;
    }
    locks.retain(|_, lock| lock.strong_count() > 0);
    let lock = Arc::new(tokio::sync::Mutex::new(()));
    locks.insert(key, Arc::downgrade(&lock));
    lock
}

/// The state, Oore fraction and store fraction a promoted release starts at.
/// Play's internal track has no staged rollouts.
fn initial_rollout(environment: &ProjectEnvironment) -> (ReleaseState, f64, Option<f64>) {
    let first = environment
        .rollout_policy
        .steps
        .first()
        .copied()
        .unwrap_or(1.0);
    if environment.track == "internal" || first >= 1.0 {
        (ReleaseState::Complete, 1.0, None)
    } else {
        (ReleaseState::Rolling, first, Some(first))
    }
}

/// The release a build is promoted from: its newest Google Play release
/// outside the target environment that still holds a live version.
async fn find_source_release(
    pool: &SqlitePool,
    build_id: &str,
    environment_id: &str,
) -> Result<Option<Release>, EnvironmentError> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{RELEASE_SELECT} WHERE r.build_id = ?1 AND r.store = 'google_play' \
         AND r.state IN ('rolling', 'paused', 'complete') AND r.version_code IS NOT NULL \
         AND (r.environment_id IS NULL OR r.environment_id <> ?2) \
         ORDER BY r.created_at DESC, r.id LIMIT 1"
    )))
    .bind(build_id)
    .bind(environment_id)
    .fetch_optional(pool)
    .await?;
    row.map(|row| row_to_release(&row).map_err(EnvironmentError::from))
        .transpose()
}

/// Refuse a build that is already in, or on its way into, the environment.
/// Runs under the promotion lock, so a `promoting` row found here belongs to
/// a call that never finished, such as one cut short by a restart.
async fn ensure_not_promoted(
    pool: &SqlitePool,
    build_id: &str,
    environment: &ProjectEnvironment,
    skip_promotion: Option<&str>,
) -> Result<(), EnvironmentError> {
    let existing: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM releases WHERE build_id = ?1 AND environment_id = ?2 AND state <> 'failed'",
    )
    .bind(build_id)
    .bind(&environment.id)
    .fetch_optional(pool)
    .await?;
    if existing.is_some() {
        return Err(EnvironmentError::conflict(
            "already_in_environment",
            format!("This build already has a release in {}", environment.name),
        ));
    }
    let open: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, state FROM environment_promotions \
         WHERE build_id = ?1 AND environment_id = ?2 AND state IN ('pending', 'promoting')",
    )
    .bind(build_id)
    .bind(&environment.id)
    .fetch_all(pool)
    .await?;
    for (id, state) in open {
        if Some(id.as_str()) == skip_promotion {
            continue;
        }
        if state == "pending" {
            return Err(EnvironmentError::conflict(
                "promotion_pending",
                format!(
                    "A promotion of this build into {} is already waiting for approval",
                    environment.name
                ),
            ));
        }
        mark_failed(pool, &id, STALE_PROMOTING).await?;
    }
    Ok(())
}

const STALE_PROMOTING: &str =
    "Oore stopped before it recorded the outcome; check the track in Play Console";
const ENVIRONMENT_DELETED: &str = "environment deleted";
const INTERRUPTED_BY_RESTART: &str =
    "Interrupted by daemon restart; check the track in Play Console before promoting again";

/// Mark every `promoting` row failed. Called once at daemon start, before
/// requests are served, when no promotion can be in flight.
pub async fn fail_interrupted_promotions(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
    let ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM environment_promotions WHERE state = 'promoting'")
            .fetch_all(pool)
            .await?;
    for id in &ids {
        mark_failed(pool, id, INTERRUPTED_BY_RESTART).await?;
        audit(
            pool,
            &ReleaseActor::System,
            "promotion_failed",
            "environment_promotion",
            id,
            serde_json::json!({ "error": INTERRUPTED_BY_RESTART }),
        )
        .await;
    }
    Ok(ids.len() as u64)
}

async fn mark_failed(
    pool: &SqlitePool,
    promotion_id: &str,
    message: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE environment_promotions SET state = 'failed', error = ?1, updated_at = ?2 WHERE id = ?3",
    )
    .bind(message)
    .bind(now_unix())
    .bind(promotion_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Ask to put `build_id` into an environment. Without an approval rule the
/// store change happens now; with one, the promotion waits as `pending`.
///
/// Promoting straight into an environment takes the maintainer role. When
/// the environment requires approval, a developer may ask, and a maintainer
/// approves.
pub(crate) async fn request_promotion(
    deps: ReleaseDeps<'_>,
    project_id: &str,
    role: &EffectiveProjectRole,
    actor: &ReleaseActor,
    request: CreatePromotionRequest,
) -> Result<EnvironmentPromotion, EnvironmentError> {
    let environment = load_environment(deps.pool, project_id, &request.environment_id).await?;
    let permission = if environment.approval_required {
        ProjectPermission::OperateRelease
    } else {
        ProjectPermission::ManageReleases
    };
    if !check_project_permission(role, permission) {
        return Err(EnvironmentError::Forbidden(
            if environment.approval_required {
                "You do not have permission to promote builds in this project".into()
            } else {
                format!(
                    "Promoting into {} takes the maintainer role. A maintainer can require approval on it so developers can ask.",
                    environment.name
                )
            },
        ));
    }
    if !environment.promotable {
        return Err(EnvironmentError::conflict(
            "promotion_unsupported",
            "Builds cannot be promoted into App Store environments yet",
        ));
    }
    let build_status: String =
        sqlx::query_scalar("SELECT status FROM builds WHERE id = ?1 AND project_id = ?2")
            .bind(&request.build_id)
            .bind(project_id)
            .fetch_optional(deps.pool)
            .await?
            .ok_or(EnvironmentError::NotFound("Build not found"))?;
    if build_status != "succeeded" {
        return Err(EnvironmentError::conflict(
            "build_not_succeeded",
            "Only a succeeded build can be promoted",
        ));
    }
    let reason = clean_reason(request.reason.as_deref());

    let lock = promotion_lock(&request.build_id, &environment.id);
    let _guard = lock.lock().await;
    ensure_not_promoted(deps.pool, &request.build_id, &environment, None).await?;
    let source = find_source_release(deps.pool, &request.build_id, &environment.id)
        .await?
        .ok_or_else(|| {
            EnvironmentError::conflict(
                "no_source_release",
                "This build has no live Google Play release to promote. Publish it to a track first.",
            )
        })?;

    let id = Uuid::new_v4().to_string();
    let now = now_unix();
    let state = if environment.approval_required {
        PromotionState::Pending
    } else {
        PromotionState::Promoting
    };
    sqlx::query(
        "INSERT INTO environment_promotions (id, project_id, build_id, environment_id, \
         source_release_id, state, reason, requested_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
    )
    .bind(&id)
    .bind(project_id)
    .bind(&request.build_id)
    .bind(&environment.id)
    .bind(&source.id)
    .bind(state.as_str())
    .bind(&reason)
    .bind(actor_id(actor))
    .bind(now)
    .execute(deps.pool)
    .await?;
    audit(
        deps.pool,
        actor,
        "promotion_requested",
        "environment_promotion",
        &id,
        serde_json::json!({
            "actor_email": actor_email(actor),
            "project_id": project_id,
            "build_id": &request.build_id,
            "environment": environment.key,
            "source_release_id": source.id,
            "source_track": source.track,
            "approval_required": environment.approval_required,
            "reason": reason,
        }),
    )
    .await;

    if state == PromotionState::Promoting {
        execute_promotion(deps, &id, &environment, &source, actor, reason.as_deref()).await?;
    }
    load_promotion(deps.pool, project_id, &id, role).await
}

/// Approve or reject a pending promotion. Approving makes the store change
/// as the approver.
pub(crate) async fn decide_promotion(
    deps: ReleaseDeps<'_>,
    project_id: &str,
    promotion_id: &str,
    role: &EffectiveProjectRole,
    actor: &ReleaseActor,
    approve: bool,
    reason: Option<&str>,
) -> Result<EnvironmentPromotion, EnvironmentError> {
    if !check_project_permission(role, ProjectPermission::ManageReleases) {
        return Err(EnvironmentError::Forbidden(
            "Approving a promotion takes the maintainer role".into(),
        ));
    }
    let promotion = load_promotion(deps.pool, project_id, promotion_id, role).await?;
    let environment = load_environment(deps.pool, project_id, &promotion.environment_id).await?;
    let lock = promotion_lock(&promotion.build_id, &environment.id);
    let _guard = lock.lock().await;
    let reason = clean_reason(reason);

    let to_state = if approve {
        PromotionState::Promoting
    } else {
        PromotionState::Rejected
    };
    let now = now_unix();
    let updated = sqlx::query(
        "UPDATE environment_promotions SET state = ?1, decided_by = ?2, decided_at = ?3, \
         decision_reason = ?4, updated_at = ?3 WHERE id = ?5 AND state = 'pending'",
    )
    .bind(to_state.as_str())
    .bind(actor_id(actor))
    .bind(now)
    .bind(&reason)
    .bind(promotion_id)
    .execute(deps.pool)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(EnvironmentError::conflict(
            "promotion_not_pending",
            "This promotion is not waiting for approval",
        ));
    }
    let requested_by: Option<String> =
        sqlx::query_scalar("SELECT requested_by FROM environment_promotions WHERE id = ?1")
            .bind(promotion_id)
            .fetch_one(deps.pool)
            .await?;
    let decided_by = actor_id(actor);
    let self_decided = requested_by.is_some() && requested_by.as_deref() == decided_by;
    audit(
        deps.pool,
        actor,
        if approve {
            "promotion_approved"
        } else {
            "promotion_rejected"
        },
        "environment_promotion",
        promotion_id,
        serde_json::json!({
            "actor_email": actor_email(actor),
            "project_id": project_id,
            "build_id": promotion.build_id,
            "environment": environment.key,
            "requested_by": promotion.requested_by_email,
            "requested_by_id": requested_by,
            "decided_by_id": decided_by,
            "self_approval": approve && self_decided,
            "self_decided": self_decided,
            "reason": reason,
        }),
    )
    .await;

    if approve {
        // The build may have changed while the promotion waited.
        let checked = async {
            ensure_not_promoted(
                deps.pool,
                &promotion.build_id,
                &environment,
                Some(promotion_id),
            )
            .await?;
            let source =
                crate::releases::load_release(deps.pool, &promotion.source_release_id).await?;
            if !matches!(
                source.state,
                ReleaseState::Rolling | ReleaseState::Paused | ReleaseState::Complete
            ) {
                return Err(EnvironmentError::conflict(
                    "source_release_not_live",
                    format!(
                        "The release this promotion starts from is {} now",
                        source.state
                    ),
                ));
            }
            Ok(source)
        }
        .await;
        let source = match checked {
            Ok(source) => source,
            Err(error) => {
                let message = match &error {
                    EnvironmentError::Conflict { message, .. } => message.clone(),
                    _ => "Oore could not check the build before promoting it".to_string(),
                };
                mark_failed(deps.pool, promotion_id, &message).await?;
                return Err(error);
            }
        };
        let merged_reason = reason.as_deref().or(promotion.reason.as_deref());
        execute_promotion(
            deps,
            promotion_id,
            &environment,
            &source,
            actor,
            merged_reason,
        )
        .await?;
    }
    load_promotion(deps.pool, project_id, promotion_id, role).await
}

/// Make the store change for a `promoting` promotion, then record the new
/// release and mark the promotion completed.
async fn execute_promotion(
    deps: ReleaseDeps<'_>,
    promotion_id: &str,
    environment: &ProjectEnvironment,
    source: &Release,
    actor: &ReleaseActor,
    reason: Option<&str>,
) -> Result<Release, EnvironmentError> {
    let (state, fraction, store_fraction) = initial_rollout(environment);
    let target = PromotionTarget {
        track: &environment.track,
        fraction: store_fraction,
    };
    if let Err(error) = deps.store.promote(source, target).await {
        let message = short(&error);
        mark_failed(deps.pool, promotion_id, &message).await?;
        audit(
            deps.pool,
            actor,
            "promotion_failed",
            "environment_promotion",
            promotion_id,
            serde_json::json!({
                "actor_email": actor_email(actor),
                "build_id": source.build_id,
                "environment": environment.key,
                "error": message,
            }),
        )
        .await;
        return Err(EnvironmentError::Store(error));
    }

    let from = source
        .environment_name
        .clone()
        .unwrap_or_else(|| format!("the {} track", source.track));
    let note = match reason {
        Some(reason) => format!("Promoted from {from} without a new build: {reason}"),
        None => format!("Promoted from {from} without a new build"),
    };
    let recorded = async {
        let mut tx = deps.pool.begin_with("BEGIN IMMEDIATE").await?;
        let created = insert_release(
            &mut tx,
            NewRelease {
                project_id: source.project_id.clone(),
                build_id: source.build_id.clone(),
                artifact_id: source.artifact_id.clone(),
                store: source.store,
                track: environment.track.clone(),
                package_name: source.package_name.clone(),
                version_name: source.version_name.clone(),
                version_code: source.version_code,
                state,
                rollout_fraction: fraction,
                policy: environment.rollout_policy.clone(),
                environment_id: Some(environment.id.clone()),
                promoted_from_release_id: Some(source.id.clone()),
            },
            actor,
            Some(&note),
        )
        .await?;
        sqlx::query(
            "UPDATE environment_promotions SET state = 'completed', release_id = ?1, error = NULL, \
             updated_at = ?2 WHERE id = ?3",
        )
        .bind(&created.release.id)
        .bind(now_unix())
        .bind(promotion_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok::<_, EnvironmentError>(created)
    }
    .await;
    let created = match recorded {
        Ok(created) => created,
        Err(error) => {
            let message = format!(
                "Play now has version code {} on the {} track, but Oore could not record the release",
                source.version_code.unwrap_or_default(),
                environment.track
            );
            mark_failed(deps.pool, promotion_id, &message).await?;
            return Err(error);
        }
    };
    let release = finish_create(deps, created, actor).await;
    audit(
        deps.pool,
        actor,
        "promotion_completed",
        "environment_promotion",
        promotion_id,
        serde_json::json!({
            "actor_email": actor_email(actor),
            "build_id": source.build_id,
            "environment": environment.key,
            "source_release_id": source.id,
            "release_id": release.id,
            "track": release.track,
            "state": release.state,
            "rollout_fraction": release.rollout_fraction,
        }),
    )
    .await;
    info!(
        promotion_id,
        build_id = %source.build_id,
        environment = %environment.key,
        release_id = %release.id,
        "build promoted"
    );
    Ok(release)
}

// ── HTTP handlers ───────────────────────────────────────────────

async fn require_project_exists(
    state: &AppState,
    project_id: &str,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM projects WHERE id = ?1")
        .bind(project_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|error| EnvironmentError::from(error).into_api())?;
    match exists {
        Some(_) => Ok(()),
        None => Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Project not found",
        )),
    }
}

async fn role_with(
    state: &AppState,
    auth: &AuthUser,
    project_id: &str,
    permission: ProjectPermission,
) -> Result<EffectiveProjectRole, (StatusCode, Json<ApiError>)> {
    let role = project_role(state, auth, project_id).await?;
    require_project_permission(&role, ProjectPermission::Read)?;
    require_project_exists(state, project_id).await?;
    require_project_permission(&role, permission)?;
    Ok(role)
}

/// `GET /v1/projects/{project_id}/environments`
pub async fn list_project_environments(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
) -> ApiResult<ListProjectEnvironmentsResponse> {
    let role = role_with(&state, &auth, &project_id, ProjectPermission::Read).await?;
    list_environments(&state.db, &project_id, &role)
        .await
        .map(Json)
        .map_err(EnvironmentError::into_api)
}

/// `POST /v1/projects/{project_id}/environments`
pub async fn create_project_environment(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
    Json(request): Json<CreateProjectEnvironmentRequest>,
) -> ApiResult<ProjectEnvironment> {
    role_with(
        &state,
        &auth,
        &project_id,
        ProjectPermission::ManageReleases,
    )
    .await?;
    create_environment(&state.db, &project_id, &user_actor(&auth), request)
        .await
        .map(Json)
        .map_err(EnvironmentError::into_api)
}

/// `PATCH /v1/projects/{project_id}/environments/{environment_id}`
pub async fn update_project_environment(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((project_id, environment_id)): Path<(String, String)>,
    Json(request): Json<UpdateProjectEnvironmentRequest>,
) -> ApiResult<ProjectEnvironment> {
    role_with(
        &state,
        &auth,
        &project_id,
        ProjectPermission::ManageReleases,
    )
    .await?;
    update_environment(
        &state.db,
        &project_id,
        &environment_id,
        &user_actor(&auth),
        request,
    )
    .await
    .map(Json)
    .map_err(EnvironmentError::into_api)
}

/// `DELETE /v1/projects/{project_id}/environments/{environment_id}`
pub async fn delete_project_environment(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((project_id, environment_id)): Path<(String, String)>,
) -> ApiResult<OkResponse> {
    role_with(
        &state,
        &auth,
        &project_id,
        ProjectPermission::ManageReleases,
    )
    .await?;
    delete_environment(&state.db, &project_id, &environment_id, &user_actor(&auth))
        .await
        .map_err(EnvironmentError::into_api)?;
    Ok(Json(OkResponse { ok: true }))
}

/// `POST /v1/projects/{project_id}/promotions`
pub async fn promote_build(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
    Json(request): Json<CreatePromotionRequest>,
) -> ApiResult<EnvironmentPromotion> {
    let role = role_with(&state, &auth, &project_id, ProjectPermission::Read).await?;
    request_promotion(
        ReleaseDeps::from(&*state),
        &project_id,
        &role,
        &user_actor(&auth),
        request,
    )
    .await
    .map(Json)
    .map_err(EnvironmentError::into_api)
}

/// `POST /v1/projects/{project_id}/promotions/{promotion_id}/approve`
pub async fn approve_promotion(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((project_id, promotion_id)): Path<(String, String)>,
    body: Option<Json<PromotionDecisionRequest>>,
) -> ApiResult<EnvironmentPromotion> {
    decide(state, auth, project_id, promotion_id, body, true).await
}

/// `POST /v1/projects/{project_id}/promotions/{promotion_id}/reject`
pub async fn reject_promotion(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((project_id, promotion_id)): Path<(String, String)>,
    body: Option<Json<PromotionDecisionRequest>>,
) -> ApiResult<EnvironmentPromotion> {
    decide(state, auth, project_id, promotion_id, body, false).await
}

async fn decide(
    state: Arc<AppState>,
    auth: AuthUser,
    project_id: String,
    promotion_id: String,
    body: Option<Json<PromotionDecisionRequest>>,
    approve: bool,
) -> ApiResult<EnvironmentPromotion> {
    let role = role_with(&state, &auth, &project_id, ProjectPermission::Read).await?;
    let request = body.map(|Json(request)| request).unwrap_or_default();
    decide_promotion(
        ReleaseDeps::from(&*state),
        &project_id,
        &promotion_id,
        &role,
        &user_actor(&auth),
        approve,
        request.reason.as_deref(),
    )
    .await
    .map(Json)
    .map_err(EnvironmentError::into_api)
}

#[cfg(test)]
mod tests {
    use oore_contract::ProjectRole;

    use super::*;
    use crate::releases::tests::{FakeStore, new_release, pool};
    use crate::releases::{create_release, load_release, run_rollout_tick};
    use crate::scheduler::Scheduler;

    fn developer() -> (EffectiveProjectRole, ReleaseActor) {
        (
            EffectiveProjectRole::Member(ProjectRole::Developer),
            ReleaseActor::User {
                user_id: "u1".into(),
                email: "dev@example.com".into(),
            },
        )
    }

    fn maintainer() -> (EffectiveProjectRole, ReleaseActor) {
        (
            EffectiveProjectRole::Member(ProjectRole::Maintainer),
            ReleaseActor::User {
                user_id: "u2".into(),
                email: "lead@example.com".into(),
            },
        )
    }

    async fn setup() -> SqlitePool {
        let pool = pool().await;
        sqlx::query(
            "INSERT INTO users (id, email, oidc_subject, role, created_at, updated_at) \
             VALUES ('u2', 'lead@example.com', 'sub2', 'developer', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    fn environment(
        key: &str,
        track: &str,
        approval_required: bool,
    ) -> CreateProjectEnvironmentRequest {
        CreateProjectEnvironmentRequest {
            key: key.into(),
            name: format!("{}{}", key[..1].to_uppercase(), &key[1..]),
            store: ReleaseStore::GooglePlay,
            track: track.into(),
            approval_required,
            rollout_policy: Some(ReleasePolicy {
                steps: vec![0.1, 0.5, 1.0],
                interval_hours: 12.0,
                ..ReleasePolicy::default()
            }),
        }
    }

    /// A runner upload to the internal track, recorded as complete.
    async fn internal_upload(deps: ReleaseDeps<'_>) -> Release {
        create_release(
            deps,
            NewRelease {
                state: ReleaseState::Complete,
                rollout_fraction: 1.0,
                ..new_release("internal", ReleasePolicy::default())
            },
            &ReleaseActor::System,
            Some("uploaded by the runner"),
        )
        .await
        .unwrap()
    }

    async fn count(pool: &SqlitePool, sql: &str) -> i64 {
        sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_string()))
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn promoting_to_production_creates_the_release_with_its_policy_without_a_build() {
        let pool = setup().await;
        let scheduler = Scheduler::new(16);
        let store = FakeStore::default();
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let (role, actor) = maintainer();
        let internal = create_environment(
            &pool,
            "p1",
            &actor,
            environment("internal", "internal", false),
        )
        .await
        .unwrap();
        let production = create_environment(
            &pool,
            "p1",
            &actor,
            environment("production", "production", false),
        )
        .await
        .unwrap();
        assert_eq!((internal.position, production.position), (0, 1));

        let source = internal_upload(deps).await;
        // An upload to a track an environment targets lands in it.
        assert_eq!(source.environment_id.as_deref(), Some(internal.id.as_str()));
        let builds_before = count(&pool, "SELECT COUNT(*) FROM builds").await;

        let promotion = request_promotion(
            deps,
            "p1",
            &role,
            &actor,
            CreatePromotionRequest {
                build_id: "b1".into(),
                environment_id: production.id.clone(),
                reason: Some("QA signed off".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(promotion.state, PromotionState::Completed);
        assert_eq!(
            promotion.source_environment_name.as_deref(),
            Some("Internal")
        );
        assert_eq!(
            *store.promotions.lock().unwrap(),
            vec![(source.id.clone(), "production".to_string(), Some(0.1))]
        );

        let release = load_release(&pool, promotion.release_id.as_deref().unwrap())
            .await
            .unwrap();
        assert_eq!(release.build_id, "b1");
        assert_eq!(release.track, "production");
        assert_eq!(release.version_code, Some(120));
        assert_eq!(
            release.environment_id.as_deref(),
            Some(production.id.as_str())
        );
        assert_eq!(release.environment_name.as_deref(), Some("Production"));
        assert_eq!(
            release.promoted_from_release_id.as_deref(),
            Some(source.id.as_str())
        );
        assert_eq!(release.state, ReleaseState::Rolling);
        assert_eq!(release.rollout_fraction, 0.1);
        assert_eq!(release.policy, production.rollout_policy);
        assert_eq!(
            count(&pool, "SELECT COUNT(*) FROM builds").await,
            builds_before
        );

        // The rollout monitor drives it on the environment's schedule.
        let due = release.next_step_at.unwrap();
        assert_eq!(due - release.created_at, 12 * 3600);
        run_rollout_tick(deps, due, 100).await.unwrap();
        assert_eq!(
            load_release(&pool, &release.id)
                .await
                .unwrap()
                .rollout_fraction,
            0.5
        );

        for action in ["promotion_requested", "promotion_completed"] {
            let audits = count(
                &pool,
                &format!(
                    "SELECT COUNT(*) FROM audit_logs WHERE action = '{action}' AND actor_id = 'u2' \
                     AND resource_type = 'environment_promotion'"
                ),
            )
            .await;
            assert_eq!(audits, 1, "{action}");
        }

        // Promoting the same build again is refused.
        let again = request_promotion(
            deps,
            "p1",
            &role,
            &actor,
            CreatePromotionRequest {
                build_id: "b1".into(),
                environment_id: production.id.clone(),
                reason: None,
            },
        )
        .await;
        assert!(matches!(
            again,
            Err(EnvironmentError::Conflict {
                code: "already_in_environment",
                ..
            })
        ));
    }

    #[tokio::test]
    async fn an_approval_holds_the_store_change_until_a_maintainer_approves() {
        let pool = setup().await;
        let scheduler = Scheduler::new(16);
        let store = FakeStore::default();
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let (lead_role, lead) = maintainer();
        let (dev_role, dev) = developer();
        let production = create_environment(
            &pool,
            "p1",
            &lead,
            environment("production", "production", true),
        )
        .await
        .unwrap();
        internal_upload(deps).await;
        let request = || CreatePromotionRequest {
            build_id: "b1".into(),
            environment_id: production.id.clone(),
            reason: None,
        };

        let pending = request_promotion(deps, "p1", &dev_role, &dev, request())
            .await
            .unwrap();
        assert_eq!(pending.state, PromotionState::Pending);
        assert!(!pending.can_decide);
        assert!(store.promotions.lock().unwrap().is_empty());
        assert!(matches!(
            request_promotion(deps, "p1", &dev_role, &dev, request()).await,
            Err(EnvironmentError::Conflict {
                code: "promotion_pending",
                ..
            })
        ));
        assert!(matches!(
            decide_promotion(deps, "p1", &pending.id, &dev_role, &dev, true, None).await,
            Err(EnvironmentError::Forbidden(_))
        ));

        let approved =
            decide_promotion(deps, "p1", &pending.id, &lead_role, &lead, true, Some("Go"))
                .await
                .unwrap();
        assert_eq!(approved.state, PromotionState::Completed);
        assert_eq!(
            approved.decided_by_email.as_deref(),
            Some("lead@example.com")
        );
        assert_eq!(store.promotions.lock().unwrap().len(), 1);
        let release = load_release(&pool, approved.release_id.as_deref().unwrap())
            .await
            .unwrap();
        // The promotion was the approval; the release itself is not gated again.
        assert!(!release.awaiting_approval);
        assert!(matches!(
            decide_promotion(deps, "p1", &pending.id, &lead_role, &lead, true, None).await,
            Err(EnvironmentError::Conflict {
                code: "promotion_not_pending",
                ..
            })
        ));
    }

    #[tokio::test]
    async fn a_rejected_promotion_never_reaches_the_store() {
        let pool = setup().await;
        let scheduler = Scheduler::new(16);
        let store = FakeStore::default();
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let (lead_role, lead) = maintainer();
        let (dev_role, dev) = developer();
        let beta = create_environment(&pool, "p1", &lead, environment("beta", "open", true))
            .await
            .unwrap();
        internal_upload(deps).await;
        let pending = request_promotion(
            deps,
            "p1",
            &dev_role,
            &dev,
            CreatePromotionRequest {
                build_id: "b1".into(),
                environment_id: beta.id.clone(),
                reason: None,
            },
        )
        .await
        .unwrap();
        let rejected = decide_promotion(
            deps,
            "p1",
            &pending.id,
            &lead_role,
            &lead,
            false,
            Some("crash in onboarding"),
        )
        .await
        .unwrap();
        assert_eq!(rejected.state, PromotionState::Rejected);
        assert_eq!(
            rejected.decision_reason.as_deref(),
            Some("crash in onboarding")
        );
        assert!(store.promotions.lock().unwrap().is_empty());
        assert_eq!(count(&pool, "SELECT COUNT(*) FROM releases").await, 1);
    }

    #[tokio::test]
    async fn developers_cannot_promote_straight_into_an_environment() {
        let pool = setup().await;
        let scheduler = Scheduler::new(16);
        let store = FakeStore::default();
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let (_, lead) = maintainer();
        let (dev_role, dev) = developer();
        let production = create_environment(
            &pool,
            "p1",
            &lead,
            environment("production", "production", false),
        )
        .await
        .unwrap();
        internal_upload(deps).await;
        let result = request_promotion(
            deps,
            "p1",
            &dev_role,
            &dev,
            CreatePromotionRequest {
                build_id: "b1".into(),
                environment_id: production.id,
                reason: None,
            },
        )
        .await;
        assert!(matches!(result, Err(EnvironmentError::Forbidden(_))));
        let viewer = EffectiveProjectRole::Member(ProjectRole::Viewer);
        let list = list_environments(&pool, "p1", &viewer).await.unwrap();
        assert_eq!(list.environments.len(), 1);
        assert!(list.promotions.is_empty());
    }

    #[tokio::test]
    async fn a_store_rejection_fails_the_promotion_and_allows_a_retry() {
        let pool = setup().await;
        let scheduler = Scheduler::new(16);
        let store = FakeStore::default();
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let (role, actor) = maintainer();
        let internal = create_environment(
            &pool,
            "p1",
            &actor,
            environment("internal", "internal", false),
        )
        .await
        .unwrap();
        let production = create_environment(
            &pool,
            "p1",
            &actor,
            environment("production", "production", false),
        )
        .await
        .unwrap();
        internal_upload(deps).await;
        let request = || CreatePromotionRequest {
            build_id: "b1".into(),
            environment_id: production.id.clone(),
            reason: None,
        };

        *store.promote_fails.lock().unwrap() = true;
        let failed = request_promotion(deps, "p1", &role, &actor, request()).await;
        assert!(matches!(failed, Err(EnvironmentError::Store(_))));
        let list = list_environments(&pool, "p1", &role).await.unwrap();
        assert_eq!(list.promotions.len(), 1);
        assert_eq!(list.promotions[0].state, PromotionState::Failed);
        assert!(
            list.promotions[0]
                .error
                .as_deref()
                .unwrap()
                .contains("lower than production")
        );
        assert_eq!(count(&pool, "SELECT COUNT(*) FROM releases").await, 1);

        *store.promote_fails.lock().unwrap() = false;
        let retried = request_promotion(deps, "p1", &role, &actor, request())
            .await
            .unwrap();
        assert_eq!(retried.state, PromotionState::Completed);

        // Internal has no staged rollout: a promotion back there is complete.
        sqlx::query(
            "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, queued_at, created_at, updated_at) \
             VALUES ('b2', 'p1', 'pl1', 8, 'succeeded', 'manual', 0, 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        create_release(
            deps,
            NewRelease {
                build_id: "b2".into(),
                version_code: Some(121),
                ..new_release("open", ReleasePolicy::default())
            },
            &ReleaseActor::System,
            None,
        )
        .await
        .unwrap();
        let back = request_promotion(
            deps,
            "p1",
            &role,
            &actor,
            CreatePromotionRequest {
                build_id: "b2".into(),
                environment_id: internal.id.clone(),
                reason: None,
            },
        )
        .await
        .unwrap();
        let release = load_release(&pool, back.release_id.as_deref().unwrap())
            .await
            .unwrap();
        assert_eq!(
            (release.state, release.rollout_fraction),
            (ReleaseState::Complete, 1.0)
        );
        assert_eq!(store.promotions.lock().unwrap().last().unwrap().2, None);
    }

    #[tokio::test]
    async fn a_build_without_a_live_play_release_cannot_be_promoted() {
        let pool = setup().await;
        let scheduler = Scheduler::new(16);
        let store = FakeStore::default();
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let (role, actor) = maintainer();
        let production = create_environment(
            &pool,
            "p1",
            &actor,
            environment("production", "production", false),
        )
        .await
        .unwrap();
        let result = request_promotion(
            deps,
            "p1",
            &role,
            &actor,
            CreatePromotionRequest {
                build_id: "b1".into(),
                environment_id: production.id,
                reason: None,
            },
        )
        .await;
        assert!(matches!(
            result,
            Err(EnvironmentError::Conflict {
                code: "no_source_release",
                ..
            })
        ));
    }

    #[tokio::test]
    async fn releases_of_projects_without_environments_are_unchanged() {
        let pool = setup().await;
        let scheduler = Scheduler::new(16);
        let store = FakeStore::default();
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let release = create_release(
            deps,
            new_release("production", ReleasePolicy::default()),
            &ReleaseActor::System,
            None,
        )
        .await
        .unwrap();
        assert_eq!(release.environment_id, None);
        assert_eq!(release.promoted_from_release_id, None);
        let json = serde_json::to_value(&release).unwrap();
        assert!(json.get("environment_id").is_none());
    }

    #[tokio::test]
    async fn environment_settings_are_validated() {
        let pool = setup().await;
        let (_, actor) = maintainer();
        let mut bad_key = environment("Prod", "production", false);
        bad_key.key = "Prod".into();
        assert!(matches!(
            create_environment(&pool, "p1", &actor, bad_key).await,
            Err(EnvironmentError::Invalid(_))
        ));
        let mut gated = environment("production", "production", false);
        gated.rollout_policy.as_mut().unwrap().approval = ReleaseApprovalGate::BeforeProduction;
        assert!(matches!(
            create_environment(&pool, "p1", &actor, gated).await,
            Err(EnvironmentError::Invalid(_))
        ));
        create_environment(
            &pool,
            "p1",
            &actor,
            environment("production", "production", false),
        )
        .await
        .unwrap();
        assert!(matches!(
            create_environment(
                &pool,
                "p1",
                &actor,
                environment("prod", "production", false)
            )
            .await,
            Err(EnvironmentError::Conflict {
                code: "environment_exists",
                ..
            })
        ));
        let beta = create_environment(&pool, "p1", &actor, environment("beta", "open", false))
            .await
            .unwrap();
        let updated = update_environment(
            &pool,
            "p1",
            &beta.id,
            &actor,
            UpdateProjectEnvironmentRequest {
                approval_required: Some(true),
                position: Some(0),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(updated.approval_required);
        assert_eq!(updated.key, "beta");
        delete_environment(&pool, "p1", &beta.id, &actor)
            .await
            .unwrap();
        assert!(matches!(
            load_environment(&pool, "p1", &beta.id).await,
            Err(EnvironmentError::NotFound(_))
        ));
    }

    async fn audit_details(
        pool: &SqlitePool,
        action: &str,
        resource_id: &str,
    ) -> serde_json::Value {
        let details: String = sqlx::query_scalar(
            "SELECT details FROM audit_logs WHERE action = ?1 AND resource_id = ?2 \
             ORDER BY id DESC LIMIT 1",
        )
        .bind(action)
        .bind(resource_id)
        .fetch_one(pool)
        .await
        .unwrap();
        serde_json::from_str(&details).unwrap()
    }

    async fn promotion_state(pool: &SqlitePool, id: &str) -> (String, Option<String>) {
        sqlx::query_as("SELECT state, COALESCE(error, decision_reason) FROM environment_promotions WHERE id = ?1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    /// A pending promotion into `environment_id`, forced into `promoting`
    /// as if the daemon stopped during the store call.
    async fn stuck_promotion(deps: ReleaseDeps<'_>, environment_id: &str) -> String {
        let (dev_role, dev) = developer();
        let pending = request_promotion(
            deps,
            "p1",
            &dev_role,
            &dev,
            CreatePromotionRequest {
                build_id: "b1".into(),
                environment_id: environment_id.into(),
                reason: None,
            },
        )
        .await
        .unwrap();
        sqlx::query("UPDATE environment_promotions SET state = 'promoting' WHERE id = ?1")
            .bind(&pending.id)
            .execute(deps.pool)
            .await
            .unwrap();
        pending.id
    }

    #[tokio::test]
    async fn the_daemon_start_fails_promotions_a_restart_interrupted() {
        let pool = setup().await;
        let scheduler = Scheduler::new(16);
        let store = FakeStore::default();
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let (_, lead) = maintainer();
        let production = create_environment(
            &pool,
            "p1",
            &lead,
            environment("production", "production", true),
        )
        .await
        .unwrap();
        internal_upload(deps).await;
        let stuck = stuck_promotion(deps, &production.id).await;

        assert_eq!(fail_interrupted_promotions(&pool).await.unwrap(), 1);
        let (state, error) = promotion_state(&pool, &stuck).await;
        assert_eq!(state, "failed");
        assert!(error.unwrap().contains("Interrupted by daemon restart"));
        assert!(
            audit_details(&pool, "promotion_failed", &stuck).await["error"]
                .as_str()
                .unwrap()
                .contains("daemon restart")
        );
        assert_eq!(fail_interrupted_promotions(&pool).await.unwrap(), 0);
        assert!(store.promotions.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn deleting_an_environment_settles_its_open_promotions() {
        let pool = setup().await;
        let scheduler = Scheduler::new(16);
        let store = FakeStore::default();
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let (_, lead) = maintainer();
        let (dev_role, dev) = developer();
        let beta = create_environment(&pool, "p1", &lead, environment("beta", "open", true))
            .await
            .unwrap();
        let production = create_environment(
            &pool,
            "p1",
            &lead,
            environment("production", "production", true),
        )
        .await
        .unwrap();
        internal_upload(deps).await;

        // A row left in `promoting` no longer blocks the delete.
        let stuck = stuck_promotion(deps, &production.id).await;
        delete_environment(&pool, "p1", &production.id, &lead)
            .await
            .unwrap();
        assert_eq!(
            audit_details(&pool, "promotion_failed", &stuck).await["environment"],
            "production"
        );
        assert_eq!(
            count(&pool, "SELECT COUNT(*) FROM environment_promotions").await,
            0
        );

        // A pending promotion is rejected, with an audit entry, first.
        let pending = request_promotion(
            deps,
            "p1",
            &dev_role,
            &dev,
            CreatePromotionRequest {
                build_id: "b1".into(),
                environment_id: beta.id.clone(),
                reason: None,
            },
        )
        .await
        .unwrap();
        delete_environment(&pool, "p1", &beta.id, &lead)
            .await
            .unwrap();
        let rejected = audit_details(&pool, "promotion_rejected", &pending.id).await;
        assert_eq!(rejected["reason"], "environment deleted");
        assert_eq!(rejected["environment"], "beta");
        assert!(store.promotions.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_promotion_running_now_still_blocks_the_delete() {
        let pool = setup().await;
        let scheduler = Scheduler::new(16);
        let store = FakeStore::default();
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let (_, lead) = maintainer();
        let production = create_environment(
            &pool,
            "p1",
            &lead,
            environment("production", "production", true),
        )
        .await
        .unwrap();
        internal_upload(deps).await;
        let stuck = stuck_promotion(deps, &production.id).await;
        let lock = promotion_lock("b1", &production.id);
        let held = lock.lock().await;
        assert!(matches!(
            delete_environment(&pool, "p1", &production.id, &lead).await,
            Err(EnvironmentError::Conflict {
                code: "promotion_in_progress",
                ..
            })
        ));
        drop(held);
        assert_eq!(promotion_state(&pool, &stuck).await.0, "promoting");
    }

    #[tokio::test]
    async fn self_approval_is_recorded_in_the_audit_log() {
        let pool = setup().await;
        let scheduler = Scheduler::new(16);
        let store = FakeStore::default();
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let (lead_role, lead) = maintainer();
        let production = create_environment(
            &pool,
            "p1",
            &lead,
            environment("production", "production", true),
        )
        .await
        .unwrap();
        internal_upload(deps).await;
        let pending = request_promotion(
            deps,
            "p1",
            &lead_role,
            &lead,
            CreatePromotionRequest {
                build_id: "b1".into(),
                environment_id: production.id.clone(),
                reason: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(pending.state, PromotionState::Pending);
        decide_promotion(deps, "p1", &pending.id, &lead_role, &lead, true, None)
            .await
            .unwrap();
        let details = audit_details(&pool, "promotion_approved", &pending.id).await;
        assert_eq!(details["self_approval"], true);
        assert_eq!(details["requested_by_id"], "u2");
        assert_eq!(details["decided_by_id"], "u2");
    }

    #[tokio::test]
    async fn an_environment_with_releases_keeps_its_track() {
        let pool = setup().await;
        let scheduler = Scheduler::new(16);
        let store = FakeStore::default();
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let (_, lead) = maintainer();
        let internal = create_environment(
            &pool,
            "p1",
            &lead,
            environment("internal", "internal", false),
        )
        .await
        .unwrap();
        let unused = create_environment(&pool, "p1", &lead, environment("beta", "open", false))
            .await
            .unwrap();
        internal_upload(deps).await;
        let moved = |track: &str| UpdateProjectEnvironmentRequest {
            track: Some(track.into()),
            ..Default::default()
        };

        let refused = update_environment(&pool, "p1", &internal.id, &lead, moved("alpha")).await;
        let Err(error) = refused else {
            panic!("the track change was accepted");
        };
        let (status, Json(body)) = error.into_api();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body.error.contains("cannot change"), "{}", body.error);
        // Other settings still change, and so does the track of an unused one.
        update_environment(
            &pool,
            "p1",
            &internal.id,
            &lead,
            UpdateProjectEnvironmentRequest {
                name: Some("QA".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let retracked = update_environment(&pool, "p1", &unused.id, &lead, moved("closed"))
            .await
            .unwrap();
        assert_eq!(retracked.track, "closed");
    }
}
