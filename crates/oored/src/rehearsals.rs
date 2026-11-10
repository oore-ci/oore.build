//! Release rehearsals: a build that signs and checks every store submission
//! the real run would make, and submits nothing.
//!
//! The runner checks what only it can (Google Play validates a staged edit
//! that is then discarded; `altool --validate-app` checks the signed IPA)
//! and reports per store. Oore adds what it reads from the store itself
//! (store readiness) and keeps the result on the build.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use oore_contract::{
    ApiError, BuildRehearsalResponse, PublishStore, ReadinessLevel, RehearsalCheck, ReleaseStore,
    RunnerRehearsalReport, RunnerRehearsalReportResponse, StoreReadinessItem,
};
use sqlx::{Row, SqlitePool};
use tracing::{error, info};

use crate::AppState;
use crate::extractors::AuthUser;
use crate::project_rbac::{
    ProjectPermission, require_project_permission, resolve_effective_project_role,
};
use crate::runners::{RunnerAuth, require_active_job_signing_grant};
use crate::util::{api_err, now_unix};

type ApiFailure = (StatusCode, Json<ApiError>);
type ApiResult<T> = Result<Json<T>, ApiFailure>;

const MAX_CHECKS: usize = 50;
const MAX_TITLE_CHARS: usize = 200;
const MAX_DETAIL_CHARS: usize = 4_000;

fn store_failure(error: sqlx::Error) -> ApiFailure {
    error!(%error, "rehearsal store error");
    api_err(
        StatusCode::INTERNAL_SERVER_ERROR,
        "store_error",
        "Failed to record the rehearsal",
    )
}

fn store_name(store: ReleaseStore) -> &'static str {
    match store {
        ReleaseStore::AppStore => "app_store",
        ReleaseStore::GooglePlay => "google_play",
    }
}

fn parse_store(value: &str) -> ReleaseStore {
    if value == "app_store" {
        ReleaseStore::AppStore
    } else {
        ReleaseStore::GooglePlay
    }
}

fn level_name(level: ReadinessLevel) -> &'static str {
    match level {
        ReadinessLevel::Ok => "ok",
        ReadinessLevel::Warn => "warn",
        ReadinessLevel::Block => "block",
    }
}

fn parse_level(value: &str) -> ReadinessLevel {
    match value {
        "block" => ReadinessLevel::Block,
        "warn" => ReadinessLevel::Warn,
        _ => ReadinessLevel::Ok,
    }
}

fn level_rank(level: ReadinessLevel) -> u8 {
    match level {
        ReadinessLevel::Block => 0,
        ReadinessLevel::Warn => 1,
        ReadinessLevel::Ok => 2,
    }
}

fn clip(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

/// The runner's checks for a store, then Oore's own readiness findings for
/// it; a readiness finding replaces the runner check with the same code.
fn merge_checks(
    store: ReleaseStore,
    runner: Vec<RehearsalCheck>,
    readiness: Vec<StoreReadinessItem>,
) -> Vec<StoreReadinessItem> {
    let mut checks: Vec<StoreReadinessItem> = runner
        .into_iter()
        .take(MAX_CHECKS)
        .map(|check| StoreReadinessItem {
            store,
            code: clip(&check.code, 64),
            level: check.level,
            title: clip(&check.title, MAX_TITLE_CHARS),
            detail: check.detail.map(|detail| clip(&detail, MAX_DETAIL_CHARS)),
            console_url: None,
            remembered: false,
        })
        .collect();
    for item in readiness.into_iter().filter(|item| item.store == store) {
        checks.retain(|check| check.code != item.code);
        checks.push(item);
    }
    checks.sort_by_key(|check| level_rank(check.level));
    checks
}

async fn save_checks(
    pool: &SqlitePool,
    build_id: &str,
    store: ReleaseStore,
    checks: &[StoreReadinessItem],
) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM build_rehearsal_checks WHERE build_id = ?1 AND store = ?2")
        .bind(build_id)
        .bind(store_name(store))
        .execute(&mut *tx)
        .await?;
    let now = now_unix();
    for (position, check) in checks.iter().enumerate() {
        sqlx::query(
            "INSERT INTO build_rehearsal_checks
                (build_id, store, position, code, level, title, detail, console_url, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .bind(build_id)
        .bind(store_name(store))
        .bind(position as i64)
        .bind(&check.code)
        .bind(level_name(check.level))
        .bind(&check.title)
        .bind(&check.detail)
        .bind(&check.console_url)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

async fn load_checks(
    pool: &SqlitePool,
    build_id: &str,
) -> sqlx::Result<(Vec<StoreReadinessItem>, Vec<ReleaseStore>)> {
    let rows = sqlx::query(
        "SELECT store, code, level, title, detail, console_url
         FROM build_rehearsal_checks WHERE build_id = ?1
         ORDER BY store, position",
    )
    .bind(build_id)
    .fetch_all(pool)
    .await?;
    let mut stores = Vec::new();
    let mut checks: Vec<StoreReadinessItem> = rows
        .iter()
        .map(|row| {
            let store = parse_store(&row.get::<String, _>("store"));
            if !stores.contains(&store) {
                stores.push(store);
            }
            StoreReadinessItem {
                store,
                code: row.get("code"),
                level: parse_level(&row.get::<String, _>("level")),
                title: row.get("title"),
                detail: row.get("detail"),
                console_url: row.get("console_url"),
                remembered: false,
            }
        })
        .collect();
    checks.sort_by_key(|check| level_rank(check.level));
    Ok((checks, stores))
}

/// `POST /v1/runners/{runner_id}/jobs/{job_id}/rehearsal` — what a
/// rehearsal found for one store. The reply is every finding for that store,
/// with Oore's own readiness checks added; the runner fails the step when
/// any of them blocks.
pub async fn report_job_rehearsal(
    State(state): State<Arc<AppState>>,
    Path((runner_id, job_id)): Path<(String, String)>,
    runner_auth: RunnerAuth,
    headers: HeaderMap,
    Json(report): Json<RunnerRehearsalReport>,
) -> ApiResult<RunnerRehearsalReportResponse> {
    if runner_auth.runner_id != runner_id {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "runner_mismatch",
            "Runner token does not match the requested runner ID",
        ));
    }
    let pipeline_id =
        require_active_job_signing_grant(&state.db, &job_id, &runner_id, &headers).await?;
    let snapshot: Option<String> =
        sqlx::query_scalar("SELECT config_snapshot FROM builds WHERE id = ?1")
            .bind(&job_id)
            .fetch_optional(&state.db)
            .await
            .map_err(store_failure)?;
    let is_rehearsal = snapshot
        .and_then(|snapshot| serde_json::from_str::<serde_json::Value>(&snapshot).ok())
        .is_some_and(|snapshot| crate::builds::snapshot_rehearsal(&snapshot));
    if !is_rehearsal {
        return Err(api_err(
            StatusCode::CONFLICT,
            "not_a_rehearsal",
            "This build is not a release rehearsal",
        ));
    }
    let store = match report.store {
        PublishStore::GooglePlay => ReleaseStore::GooglePlay,
        PublishStore::AppStore => ReleaseStore::AppStore,
    };
    let readiness = crate::store_readiness::pipeline_readiness_items(&state, &pipeline_id)
        .await
        .map_err(store_failure)?;
    let checks = merge_checks(store, report.checks, readiness);
    save_checks(&state.db, &job_id, store, &checks)
        .await
        .map_err(store_failure)?;
    info!(
        build_id = %job_id,
        store = store_name(store),
        blocking = checks.iter().filter(|c| c.level == ReadinessLevel::Block).count(),
        "runner reported a release rehearsal"
    );
    Ok(Json(RunnerRehearsalReportResponse { checks }))
}

/// `GET /v1/builds/{build_id}/rehearsal` — what a rehearsal build found.
pub async fn get_build_rehearsal(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
) -> ApiResult<BuildRehearsalResponse> {
    let project_id: String = sqlx::query_scalar("SELECT project_id FROM builds WHERE id = ?1")
        .bind(&build_id)
        .fetch_optional(&state.db)
        .await
        .map_err(store_failure)?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Build not found"))?;
    let effective = resolve_effective_project_role(
        &state.db,
        &auth.0.user_id,
        &auth.0.role,
        &project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, ProjectPermission::Read)?;
    let (checks, stores) = load_checks(&state.db, &build_id)
        .await
        .map_err(store_failure)?;
    Ok(Json(BuildRehearsalResponse {
        build_id,
        checks,
        stores,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(code: &str, level: ReadinessLevel) -> RehearsalCheck {
        RehearsalCheck {
            code: code.to_string(),
            level,
            title: code.to_string(),
            detail: None,
        }
    }

    fn readiness(store: ReleaseStore, code: &str, level: ReadinessLevel) -> StoreReadinessItem {
        StoreReadinessItem {
            store,
            code: code.to_string(),
            level,
            title: code.to_string(),
            detail: None,
            console_url: Some("https://appstoreconnect.apple.com".to_string()),
            remembered: false,
        }
    }

    #[test]
    fn merge_puts_blocks_first_and_keeps_only_this_store() {
        let checks = merge_checks(
            ReleaseStore::AppStore,
            vec![
                check("asc_validate", ReadinessLevel::Ok),
                check("app_icon", ReadinessLevel::Warn),
            ],
            vec![
                readiness(ReleaseStore::AppStore, "age_rating", ReadinessLevel::Block),
                readiness(
                    ReleaseStore::GooglePlay,
                    "store_refusal",
                    ReadinessLevel::Block,
                ),
            ],
        );
        let codes: Vec<&str> = checks.iter().map(|c| c.code.as_str()).collect();
        assert_eq!(codes, ["age_rating", "app_icon", "asc_validate"]);
    }

    #[test]
    fn a_store_finding_replaces_the_runner_check_with_its_code() {
        let checks = merge_checks(
            ReleaseStore::GooglePlay,
            vec![check("store_refusal", ReadinessLevel::Ok)],
            vec![readiness(
                ReleaseStore::GooglePlay,
                "store_refusal",
                ReadinessLevel::Block,
            )],
        );
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].level, ReadinessLevel::Block);
    }

    #[tokio::test]
    async fn checks_round_trip_per_store() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        for statement in [
            "INSERT INTO users (id, email, oidc_subject, created_at, updated_at) \
             VALUES ('u1', 'dev@example.com', 'sub', 0, 0)",
            "INSERT INTO projects (id, name, default_branch, created_by, created_at, updated_at) \
             VALUES ('p1', 'App', 'main', 'u1', 0, 0)",
            "INSERT INTO pipelines (id, project_id, name, created_at, updated_at) \
             VALUES ('pl1', 'p1', 'Release', 0, 0)",
            "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
             branch, commit_sha, config_snapshot, queued_at, created_at, updated_at) \
             VALUES ('b1', 'p1', 'pl1', 1, 'running', 'manual', 'main', 'c1', \
             '{\"rehearsal\":true}', 0, 0, 0)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        let play = merge_checks(
            ReleaseStore::GooglePlay,
            vec![check("play_validate", ReadinessLevel::Ok)],
            Vec::new(),
        );
        save_checks(&pool, "b1", ReleaseStore::GooglePlay, &play)
            .await
            .unwrap();
        let apple = merge_checks(
            ReleaseStore::AppStore,
            vec![check("asc_validate", ReadinessLevel::Block)],
            Vec::new(),
        );
        save_checks(&pool, "b1", ReleaseStore::AppStore, &apple)
            .await
            .unwrap();
        // A second report for a store replaces its checks.
        save_checks(&pool, "b1", ReleaseStore::AppStore, &apple)
            .await
            .unwrap();
        let (checks, stores) = load_checks(&pool, "b1").await.unwrap();
        assert_eq!(checks.len(), 2);
        assert_eq!(checks[0].code, "asc_validate");
        assert_eq!(stores.len(), 2);
    }
}
