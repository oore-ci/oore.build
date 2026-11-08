//! Store readiness: what would stop a pipeline's next store submission.
//!
//! Two sources. Some blockers can be read from App Store Connect before
//! anything is uploaded (unanswered age rating questions, missing App Review
//! contact details, another submission holding the review queue). Others
//! Apple or Google only name when they refuse a request; those are
//! remembered per app here until a later submission goes through, so the
//! next build's check shows them before the upload.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use oore_contract::{
    ApiError, AppStoreDestination, PublishPreflight, ReadinessLevel, ReleaseStore,
    StoreReadinessItem, StoreReadinessResponse,
};
use sqlx::SqlitePool;
use tracing::{error, warn};

use crate::AppState;
use crate::app_store_connect::AppStoreConnect;
use crate::app_store_publish;
use crate::extractors::AuthUser;
use crate::pipeline_publish;
use crate::project_rbac::{ProjectPermission, require_pipeline_project_permission};
use crate::releases::console_url;
use crate::util::{api_err, now_unix};

type ApiFailure = (StatusCode, Json<ApiError>);

/// A stable code and a short title for a store's refusal message.
pub(crate) fn classify_refusal(message: &str) -> (&'static str, &'static str) {
    let lower = message.to_ascii_lowercase();
    let has = |needle: &str| lower.contains(needle);
    if has("age rating") || has("agerating") {
        ("age_rating", "Age rating questions are unanswered")
    } else if has("export compliance") || has("encryption") {
        ("export_compliance", "Export compliance is not answered")
    } else if has("review detail") || has("contact") || has("demo account") {
        ("review_details", "App Review contact details are missing")
    } else if has("privacy") || has("data collection") || has("data safety") {
        ("privacy", "Privacy details are incomplete")
    } else if has("content rights") || has("content right") {
        ("content_rights", "Content rights are not declared")
    } else if has("pricing") || has("price") || has("availability") {
        ("pricing", "Pricing or availability is not set")
    } else if has("agreement") || has("contract") || has("tax") || has("banking") {
        (
            "agreement",
            "An agreement, tax or banking form needs attention",
        )
    } else if has("screenshot") {
        ("screenshots", "Screenshots are missing")
    } else if has("already") && (has("version") || has("build")) {
        (
            "version_used",
            "That version or build number is already used",
        )
    } else {
        ("store_refusal", "The store refused the last submission")
    }
}

/// Remembers a store refusal for an app, replacing an earlier one with the
/// same code.
pub(crate) async fn remember_refusal(
    pool: &SqlitePool,
    project_id: &str,
    store: &str,
    app_identifier: &str,
    message: &str,
    console_url: Option<&str>,
    now: i64,
) -> sqlx::Result<()> {
    let (code, title) = classify_refusal(message);
    let detail: String = message.chars().take(1000).collect();
    sqlx::query(
        "INSERT INTO store_readiness_memory
            (project_id, store, app_identifier, code, title, detail, console_url, recorded_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT (project_id, store, app_identifier, code) DO UPDATE SET
            title = excluded.title, detail = excluded.detail,
            console_url = COALESCE(excluded.console_url, store_readiness_memory.console_url),
            recorded_at = excluded.recorded_at",
    )
    .bind(project_id)
    .bind(store)
    .bind(app_identifier)
    .bind(code)
    .bind(title)
    .bind(detail)
    .bind(console_url)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Forgets an app's remembered refusals once a submission went through.
pub(crate) async fn forget_refusals(
    pool: &SqlitePool,
    project_id: &str,
    store: &str,
    app_identifier: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "DELETE FROM store_readiness_memory
         WHERE project_id = ?1 AND store = ?2 AND app_identifier = ?3",
    )
    .bind(project_id)
    .bind(store)
    .bind(app_identifier)
    .execute(pool)
    .await?;
    Ok(())
}

/// What `preflight: auto` means for a pipeline: block once the store has
/// accepted one of its releases (the store setup is known to work, so a
/// finding is real), warn before that.
pub(crate) async fn resolve_preflight(
    pool: &SqlitePool,
    pipeline_id: &str,
    configured: PublishPreflight,
) -> PublishPreflight {
    if configured != PublishPreflight::Auto {
        return configured;
    }
    let accepted: Result<Option<i64>, _> = sqlx::query_scalar(
        "SELECT 1 FROM releases r LEFT JOIN builds b ON b.id = r.build_id
         WHERE COALESCE(r.pipeline_id, b.pipeline_id) = ?1
           AND r.state IN ('in_review', 'approved', 'rolling', 'paused', 'halted', 'complete')
         LIMIT 1",
    )
    .bind(pipeline_id)
    .fetch_optional(pool)
    .await;
    match accepted {
        Ok(Some(_)) => PublishPreflight::Block,
        Ok(None) => PublishPreflight::Warn,
        Err(error) => {
            warn!(%error, pipeline_id, "failed to resolve pre-flight level");
            PublishPreflight::Warn
        }
    }
}

#[derive(sqlx::FromRow)]
struct MemoryRow {
    store: String,
    code: String,
    title: String,
    detail: Option<String>,
    console_url: Option<String>,
}

async fn remembered(pool: &SqlitePool, pipeline_id: &str) -> sqlx::Result<Vec<StoreReadinessItem>> {
    let rows: Vec<MemoryRow> = sqlx::query_as(
        "SELECT m.store, m.code, m.title, m.detail, m.console_url
         FROM store_readiness_memory m
         JOIN pipelines p ON p.project_id = m.project_id
         WHERE p.id = ?1
           AND EXISTS (
               SELECT 1 FROM releases r LEFT JOIN builds b ON b.id = r.build_id
               WHERE COALESCE(r.pipeline_id, b.pipeline_id) = p.id
                 AND r.store = m.store AND r.package_name = m.app_identifier
           )
         ORDER BY m.recorded_at DESC",
    )
    .bind(pipeline_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| StoreReadinessItem {
            store: if row.store == "app_store" {
                ReleaseStore::AppStore
            } else {
                ReleaseStore::GooglePlay
            },
            code: row.code,
            level: ReadinessLevel::Block,
            title: row.title,
            detail: row.detail,
            console_url: row.console_url,
            remembered: true,
        })
        .collect())
}

/// Reads what App Store Connect already shows would block a submission.
async fn app_store_items(state: &AppState, pipeline_id: &str) -> Vec<StoreReadinessItem> {
    let mut items = Vec::new();
    let Ok(Some((config, _))) = app_store_publish::load_config(&state.db, pipeline_id).await else {
        return items;
    };
    if !config.enabled || config.destination != AppStoreDestination::AppStore {
        return items;
    }
    let item = |code: &str, level, title: &str, detail: Option<String>, url: Option<String>| {
        StoreReadinessItem {
            store: ReleaseStore::AppStore,
            code: code.to_string(),
            level,
            title: title.to_string(),
            detail,
            console_url: url,
            remembered: false,
        }
    };
    let creds = match app_store_publish::load_credentials(state, pipeline_id).await {
        Ok(creds) => creds,
        Err(error) => {
            items.push(item(
                "api_key",
                ReadinessLevel::Block,
                "No App Store Connect API key",
                Some(format!("{error:#}")),
                None,
            ));
            return items;
        }
    };
    let Some(bundle_id) = app_store_publish::pipeline_bundle_id(&state.db, pipeline_id).await
    else {
        return items;
    };
    let client = app_store_publish::app_store_http_client();
    let asc = AppStoreConnect::new(&client, &creds);
    let app_id = match asc.find_app_id(&bundle_id).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            items.push(item(
                "app_missing",
                ReadinessLevel::Block,
                "App Store Connect has no app for this bundle ID",
                Some(format!("Create the app record for {bundle_id} first.")),
                Some("https://appstoreconnect.apple.com/apps".to_string()),
            ));
            return items;
        }
        Err(error) => {
            items.push(item(
                "unreachable",
                ReadinessLevel::Warn,
                "Could not read App Store Connect",
                Some(error.0),
                None,
            ));
            return items;
        }
    };
    let version_url = console_url(ReleaseStore::AppStore, "app_store", None, Some(&app_id));
    let app_info_url = Some(format!(
        "https://appstoreconnect.apple.com/apps/{app_id}/distribution/info"
    ));
    match asc.age_rating_gaps(&app_id).await {
        Ok(gaps) if !gaps.is_empty() => items.push(item(
            "age_rating",
            ReadinessLevel::Block,
            "Age rating questions are unanswered",
            Some(format!("Unanswered: {}.", gaps.join(", "))),
            app_info_url,
        )),
        Ok(_) => {}
        Err(error) => warn!(%error, pipeline_id, "age rating check failed"),
    }
    if config.submit_for_review {
        if let Ok(Some(version)) = asc.find_editable_version(&app_id).await {
            match asc.review_detail_gaps(&version.id).await {
                Ok(gaps) if !gaps.is_empty() => items.push(item(
                    "review_details",
                    ReadinessLevel::Block,
                    "App Review contact details are missing",
                    Some(format!("Missing: {}.", gaps.join(", "))),
                    version_url.clone(),
                )),
                Ok(_) => {}
                Err(error) => warn!(%error, pipeline_id, "review details check failed"),
            }
        }
        match asc.open_submission_has_other_items(&app_id).await {
            Ok(true) => items.push(item(
                "open_submission",
                if config.cancel_previous_submission {
                    ReadinessLevel::Warn
                } else {
                    ReadinessLevel::Block
                },
                "Another review submission is open",
                Some(if config.cancel_previous_submission {
                    "Oore will cancel it before submitting (cancel_previous_submission is on)."
                        .to_string()
                } else {
                    "Apple allows one open submission per app. Cancel it in App Store Connect, or turn on cancel_previous_submission.".to_string()
                }),
                version_url,
            )),
            Ok(false) => {}
            Err(error) => warn!(%error, pipeline_id, "open submission check failed"),
        }
    }
    items
}

/// `GET /v1/pipelines/{pipeline_id}/publish/readiness`.
pub async fn get_pipeline_publish_readiness(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
) -> Result<Json<StoreReadinessResponse>, ApiFailure> {
    require_pipeline_project_permission(
        &state.db,
        &auth.0.user_id,
        &auth.0.role,
        &auth.0.auth_source,
        &pipeline_id,
        ProjectPermission::Read,
    )
    .await?;
    let store_failure = |error: sqlx::Error| {
        error!(%error, "store readiness error");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to check store readiness",
        )
    };
    let mut items = remembered(&state.db, &pipeline_id)
        .await
        .map_err(store_failure)?;
    for live in app_store_items(&state, &pipeline_id).await {
        // A live finding replaces the remembered one with the same code.
        items.retain(|item| !(item.store == live.store && item.code == live.code));
        items.push(live);
    }
    items.sort_by_key(|item| match item.level {
        ReadinessLevel::Block => 0,
        ReadinessLevel::Warn => 1,
        ReadinessLevel::Ok => 2,
    });
    let configured = match app_store_publish::load_config(&state.db, &pipeline_id).await {
        Ok(Some((config, _))) if config.enabled => config.preflight,
        _ => match pipeline_publish::load_google_play(&state.db, &pipeline_id).await {
            Ok(Some((config, _))) => config.preflight,
            _ => PublishPreflight::Auto,
        },
    };
    Ok(Json(StoreReadinessResponse {
        preflight: resolve_preflight(&state.db, &pipeline_id, configured).await,
        pipeline_id,
        items,
        checked_at: now_unix(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_refusals() {
        assert_eq!(
            classify_refusal("ENTITY_ERROR.ATTRIBUTE.REQUIRED: ageRatingDeclaration is missing").0,
            "age_rating"
        );
        assert_eq!(
            classify_refusal("You must provide App Review contact information").0,
            "review_details"
        );
        assert_eq!(classify_refusal("something else").0, "store_refusal");
    }
}
