//! Store health for the Home dashboard: Google Play crash and ANR rates with
//! their top issues, and App Store hang rate and launch time.
//!
//! The daemon discovers apps from releases and from pipelines' Google Play
//! settings, reads each store in the background with that pipeline's
//! publishing credentials, and keeps
//! the latest result so Home never waits on a store. Only the monitor reads
//! stores; a manual refresh marks rows due and wakes it.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use oore_contract::{
    ApiError, ReleaseStore, StoreAppVitals, StoreErrorIssue, StoreVital, StoreVitalKind,
    StoreVitalPoint, StoreVitalsResponse,
};
use oore_google_play::{PlayErrorIssue, PlayVitalsKind, PlayVitalsRate};
use serde_json::Value;
use sqlx::{FromRow, SqlitePool};
use tokio::sync::Notify;
use tracing::{error, warn};

use crate::AppState;
use crate::app_store_connect::AppStoreConnect;
use crate::extractors::AuthUser;
use crate::pipeline_publish::load_google_play;
use crate::secrets::resolve_secret_value;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

const MONITOR_INTERVAL: Duration = Duration::from_secs(5 * 60);
const REFRESH_AFTER_SECS: i64 = 6 * 60 * 60;
/// A failed read is often a setting someone is fixing right now.
const RETRY_AFTER_SECS: i64 = 15 * 60;
/// Rows read this recently are not read again on a manual refresh.
const MANUAL_REFRESH_COOLDOWN_SECS: i64 = 30;
const MANUAL_REFRESH_LIMIT: i64 = 20;
/// How long a manual refresh waits for the monitor before answering with
/// whatever has been read so far. Keep it well under proxy timeouts.
const MANUAL_REFRESH_WAIT: Duration = Duration::from_secs(20);
/// A claimed row is read again after this if its read never finished, for
/// example because the daemon stopped mid-read.
const CLAIM_SECS: i64 = RETRY_AFTER_SECS;

/// Wakes the monitor for rows a manual refresh marked due.
static MONITOR_WAKE: Notify = Notify::const_new();
/// Signalled each time the monitor has no due rows left.
static MONITOR_IDLE: Notify = Notify::const_new();
const REFRESH_BATCH: i64 = 5;
const TOP_ISSUES: u32 = 5;
/// Google Play's bad-behavior thresholds for user-perceived rates.
const PLAY_CRASH_THRESHOLD: f64 = 0.0109;
const PLAY_ANR_THRESHOLD: f64 = 0.0047;

#[derive(Debug, FromRow)]
struct VitalsRow {
    project_id: String,
    project_name: String,
    store: String,
    app_identifier: String,
    vitals_json: Option<String>,
    issues_json: Option<String>,
    error: Option<String>,
    refreshed_at: Option<i64>,
}

/// `GET /v1/store-vitals`
pub async fn get_store_vitals(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<StoreVitalsResponse> {
    load_vitals(&state, &auth).await
}

/// `POST /v1/store-vitals/refresh` — read the stores now for the caller's
/// apps, for example right after fixing a permission, instead of waiting for
/// the next scheduled read.
pub async fn refresh_store_vitals(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<StoreVitalsResponse> {
    let store_err = |e: sqlx::Error| {
        error!(error = %e, "failed to refresh store vitals");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to refresh store vitals",
        )
    };
    let privileged = auth.0.role == "owner" || auth.0.role == "admin";
    let marked = mark_due(&state.db, privileged, &auth.0.user_id, now_unix())
        .await
        .map_err(store_err)?;
    if marked > 0 {
        MONITOR_WAKE.notify_one();
        // A slow store must not hold the request open; whatever is still
        // being read shows up on a later load.
        let deadline = tokio::time::Instant::now() + MANUAL_REFRESH_WAIT;
        loop {
            // Listen before checking so an idle signal in between is kept.
            let idle = MONITOR_IDLE.notified();
            tokio::pin!(idle);
            idle.as_mut().enable();
            if !any_due(&state.db, now_unix()).await.map_err(store_err)?
                || tokio::time::timeout_at(deadline, idle).await.is_err()
            {
                break;
            }
        }
    }
    load_vitals(&state, &auth).await
}

async fn any_due(pool: &SqlitePool, now: i64) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM store_app_vitals WHERE next_refresh_at <= ?1)")
        .bind(now)
        .fetch_one(pool)
        .await
}

/// Mark the caller's apps due so the monitor reads them next, skipping rows
/// read in the last few seconds. Returns how many rows were marked.
async fn mark_due(
    pool: &SqlitePool,
    privileged: bool,
    user_id: &str,
    now: i64,
) -> sqlx::Result<u64> {
    discover_apps(pool).await?;
    let result = sqlx::query(
        "UPDATE store_app_vitals SET next_refresh_at = 0 WHERE rowid IN ( \
           SELECT rowid FROM store_app_vitals v \
           WHERE (?1 OR EXISTS (SELECT 1 FROM project_members pm \
             WHERE pm.project_id = v.project_id AND pm.user_id = ?2)) \
             AND (refreshed_at IS NULL OR refreshed_at < ?3) \
           ORDER BY next_refresh_at LIMIT ?4)",
    )
    .bind(privileged)
    .bind(user_id)
    .bind(now - MANUAL_REFRESH_COOLDOWN_SECS)
    .bind(MANUAL_REFRESH_LIMIT)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

async fn load_vitals(state: &AppState, auth: &AuthUser) -> ApiResult<StoreVitalsResponse> {
    let privileged = auth.0.role == "owner" || auth.0.role == "admin";
    let rows = sqlx::query_as::<_, VitalsRow>(
        "SELECT v.project_id, p.name AS project_name, v.store, v.app_identifier, v.vitals_json, \
         v.issues_json, v.error, v.refreshed_at \
         FROM store_app_vitals v JOIN projects p ON p.id = v.project_id \
         WHERE ?1 OR EXISTS (SELECT 1 FROM project_members pm \
           WHERE pm.project_id = v.project_id AND pm.user_id = ?2) \
         ORDER BY p.name COLLATE NOCASE, v.store, v.app_identifier",
    )
    .bind(privileged)
    .bind(&auth.0.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to load store vitals");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load store vitals",
        )
    })?;
    let apps = rows
        .into_iter()
        .filter_map(|row| {
            let store = match row.store.as_str() {
                "google_play" => ReleaseStore::GooglePlay,
                "app_store" => ReleaseStore::AppStore,
                _ => return None,
            };
            Some(StoreAppVitals {
                project_id: row.project_id,
                project_name: row.project_name,
                store,
                app_identifier: row.app_identifier,
                vitals: row
                    .vitals_json
                    .and_then(|raw| serde_json::from_str(&raw).ok())
                    .unwrap_or_default(),
                issues: row
                    .issues_json
                    .and_then(|raw| serde_json::from_str(&raw).ok())
                    .unwrap_or_default(),
                refreshed_at: row.refreshed_at,
                error: row.error,
            })
        })
        .collect();
    Ok(Json(StoreVitalsResponse { apps }))
}

/// Google Play apps named by a pipeline's saved settings: the expected
/// package, else the package its last passing credentials check reached.
macro_rules! configured_play_apps {
    () => {
        "SELECT p.project_id, t.pipeline_id, t.updated_at, \
           COALESCE(NULLIF(TRIM(json_extract(t.config, '$.package_name')), ''), \
             c.package_name) AS package_name \
         FROM pipeline_publish_targets t JOIN pipelines p ON p.id = t.pipeline_id \
         LEFT JOIN pipeline_google_play_checks c ON c.pipeline_id = t.pipeline_id AND c.ok = 1 \
         WHERE t.store = 'google_play'"
    };
}

/// Track every app a project has released, read through the pipeline of its
/// newest release, and every Google Play app a pipeline is set up to publish,
/// so an app shows before Oore has released it.
async fn discover_apps(pool: &SqlitePool) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO store_app_vitals (project_id, store, app_identifier, pipeline_id) \
         SELECT r.project_id, 'google_play', r.package_name, b.pipeline_id \
         FROM releases r JOIN builds b ON b.id = r.build_id \
         WHERE r.store = 'google_play' AND r.package_name IS NOT NULL \
           AND r.created_at = (SELECT MAX(r2.created_at) FROM releases r2 \
             WHERE r2.project_id = r.project_id AND r2.store = 'google_play' \
               AND r2.package_name = r.package_name) \
         ON CONFLICT (project_id, store, app_identifier) \
         DO UPDATE SET pipeline_id = excluded.pipeline_id",
    )
    .execute(pool)
    .await?;
    // A released app keeps the pipeline of its newest release; among
    // pipelines set up for the same app, the most recently saved one reads it.
    sqlx::query(concat!(
        "INSERT INTO store_app_vitals (project_id, store, app_identifier, pipeline_id) \
         SELECT project_id, 'google_play', package_name, pipeline_id FROM (",
        configured_play_apps!(),
        ") WHERE package_name IS NOT NULL \
         ORDER BY updated_at \
         ON CONFLICT (project_id, store, app_identifier) \
         DO UPDATE SET pipeline_id = excluded.pipeline_id \
         WHERE NOT EXISTS (SELECT 1 FROM releases r \
           WHERE r.project_id = excluded.project_id AND r.store = 'google_play' \
             AND r.package_name = excluded.app_identifier)"
    ))
    .execute(pool)
    .await?;
    // An app that was only configured goes away once no pipeline names it.
    sqlx::query(concat!(
        "DELETE FROM store_app_vitals AS v WHERE v.store = 'google_play' \
           AND NOT EXISTS (SELECT 1 FROM releases r WHERE r.project_id = v.project_id \
             AND r.store = 'google_play' AND r.package_name = v.app_identifier) \
           AND NOT EXISTS (SELECT 1 FROM (",
        configured_play_apps!(),
        ") c WHERE c.project_id = v.project_id AND c.package_name = v.app_identifier)"
    ))
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO store_app_vitals (project_id, store, app_identifier, pipeline_id, asc_app_id) \
         SELECT r.project_id, 'app_store', l.bundle_id, l.pipeline_id, l.asc_app_id \
         FROM app_store_release_links l JOIN releases r ON r.id = l.release_id \
         WHERE r.created_at = (SELECT MAX(r2.created_at) FROM app_store_release_links l2 \
             JOIN releases r2 ON r2.id = l2.release_id \
             WHERE r2.project_id = r.project_id AND l2.bundle_id = l.bundle_id) \
         ON CONFLICT (project_id, store, app_identifier) \
         DO UPDATE SET pipeline_id = excluded.pipeline_id, \
           asc_app_id = COALESCE(excluded.asc_app_id, store_app_vitals.asc_app_id)",
    )
    .execute(pool)
    .await?;
    Ok(())
}

struct StoreRead {
    vitals: Vec<StoreVital>,
    issues: Vec<StoreErrorIssue>,
    asc_app_id: Option<String>,
}

/// A row claimed for reading: its key, credentials and the claim's
/// `next_refresh_at`, which tells whether someone asked for a new read while
/// this one ran.
#[derive(Debug, FromRow)]
struct DueRow {
    project_id: String,
    store: String,
    app_identifier: String,
    pipeline_id: String,
    asc_app_id: Option<String>,
    claimed_until: i64,
}

/// Claim up to a batch of due rows by pushing their `next_refresh_at` out, so
/// no other pass reads them at the same time.
async fn claim_due(pool: &SqlitePool, now: i64) -> sqlx::Result<Vec<DueRow>> {
    sqlx::query_as::<_, DueRow>(
        "UPDATE store_app_vitals SET next_refresh_at = ?1 WHERE rowid IN ( \
           SELECT rowid FROM store_app_vitals WHERE next_refresh_at <= ?2 \
           ORDER BY next_refresh_at LIMIT ?3) \
         RETURNING project_id, store, app_identifier, pipeline_id, asc_app_id, \
           next_refresh_at AS claimed_until",
    )
    .bind(now + CLAIM_SECS)
    .bind(now)
    .bind(REFRESH_BATCH)
    .fetch_all(pool)
    .await
}

/// Read every due row, a batch at a time.
async fn refresh_due(state: &AppState, http: &reqwest::Client) -> sqlx::Result<()> {
    loop {
        let rows = claim_due(&state.db, now_unix()).await?;
        if rows.is_empty() {
            return Ok(());
        }
        for row in rows {
            let started = now_unix();
            let read = match row.store.as_str() {
                "google_play" => read_play(state, http, &row).await,
                _ => read_app_store(state, http, &row).await,
            };
            record_read(&state.db, &row, started, read).await?;
        }
    }
}

/// Store a read's result. A read never overwrites numbers from a read that
/// started after it, and a manual refresh requested while it ran stays due.
async fn record_read(
    pool: &SqlitePool,
    row: &DueRow,
    started: i64,
    read: anyhow::Result<StoreRead>,
) -> sqlx::Result<()> {
    let (vitals_json, issues_json, asc_app_id, error, next_refresh_at) = match read {
        Ok(read) => (
            Some(serde_json::to_string(&read.vitals).unwrap_or_default()),
            Some(serde_json::to_string(&read.issues).unwrap_or_default()),
            read.asc_app_id,
            None,
            started + REFRESH_AFTER_SECS,
        ),
        Err(error) => {
            warn!(%error, project_id = %row.project_id, app = %row.app_identifier, "store vitals read failed");
            (
                None,
                None,
                None,
                Some(format!("{error:#}")),
                started + RETRY_AFTER_SECS,
            )
        }
    };
    // On failure keep the last good numbers; the error explains why they
    // are stale.
    sqlx::query(
        "UPDATE store_app_vitals SET \
           vitals_json = COALESCE(?1, vitals_json), issues_json = COALESCE(?2, issues_json), \
           asc_app_id = COALESCE(?3, asc_app_id), error = ?4, \
           refreshed_at = CASE WHEN ?4 IS NULL THEN ?5 ELSE refreshed_at END, \
           next_refresh_at = CASE WHEN next_refresh_at = ?6 THEN ?7 ELSE next_refresh_at END \
         WHERE project_id = ?8 AND store = ?9 AND app_identifier = ?10 \
           AND (refreshed_at IS NULL OR refreshed_at <= ?5)",
    )
    .bind(vitals_json)
    .bind(issues_json)
    .bind(asc_app_id)
    .bind(error)
    .bind(started)
    .bind(row.claimed_until)
    .bind(next_refresh_at)
    .bind(&row.project_id)
    .bind(&row.store)
    .bind(&row.app_identifier)
    .execute(pool)
    .await?;
    Ok(())
}

async fn read_play(
    state: &AppState,
    http: &reqwest::Client,
    row: &DueRow,
) -> anyhow::Result<StoreRead> {
    let (config, _) = load_google_play(&state.db, &row.pipeline_id)
        .await?
        .context("the releasing pipeline has no Google Play settings")?;
    let service_account = resolve_secret_value(
        &state.db,
        &state.encryption_key,
        &row.project_id,
        Some(&row.pipeline_id),
        &config.service_account_secret,
    )
    .await?
    .with_context(|| {
        format!(
            "the service account secret {} is not set",
            config.service_account_secret
        )
    })?;
    let token = oore_google_play::access_token_with_scope(
        http,
        &service_account,
        oore_google_play::REPORTING_SCOPE,
    )
    .await?;
    let package = &row.app_identifier;
    let crash =
        oore_google_play::app_vitals_rate(http, &token, package, PlayVitalsKind::CrashRate).await?;
    let anr =
        oore_google_play::app_vitals_rate(http, &token, package, PlayVitalsKind::AnrRate).await?;
    let issues = oore_google_play::top_error_issues(http, &token, package, TOP_ISSUES).await?;
    Ok(StoreRead {
        vitals: vec![
            play_vital(StoreVitalKind::CrashRate, crash, PLAY_CRASH_THRESHOLD),
            play_vital(StoreVitalKind::AnrRate, anr, PLAY_ANR_THRESHOLD),
        ],
        issues: issues.into_iter().map(play_issue).collect(),
        asc_app_id: None,
    })
}

fn play_vital(kind: StoreVitalKind, rate: PlayVitalsRate, threshold: f64) -> StoreVital {
    StoreVital {
        kind,
        value: rate.rate_28d,
        unit: "ratio".to_string(),
        threshold: Some(threshold),
        points: rate
            .days
            .into_iter()
            .map(|day| StoreVitalPoint {
                label: day.date,
                value: day.rate,
            })
            .collect(),
    }
}

fn play_issue(issue: PlayErrorIssue) -> StoreErrorIssue {
    StoreErrorIssue {
        kind: issue.kind.to_lowercase(),
        cause: issue.cause,
        location: issue.location,
        reports: issue.reports,
        users: issue.users,
        last_reported_at: issue
            .last_report_time
            .and_then(|time| chrono::DateTime::parse_from_rfc3339(&time).ok())
            .map(|time| time.timestamp()),
        url: issue.url,
    }
}

async fn read_app_store(
    state: &AppState,
    http: &reqwest::Client,
    row: &DueRow,
) -> anyhow::Result<StoreRead> {
    let creds = crate::app_store_publish::load_credentials(state, &row.pipeline_id).await?;
    let asc = AppStoreConnect::new(http, &creds);
    let app_id = match &row.asc_app_id {
        Some(id) => id.clone(),
        None => asc
            .find_app_id(&row.app_identifier)
            .await
            .map_err(|e| anyhow::anyhow!(e.0))?
            .with_context(|| {
                format!(
                    "no App Store Connect app has bundle id {}",
                    row.app_identifier
                )
            })?,
    };
    let metrics = asc
        .perf_power_metrics(&app_id)
        .await
        .map_err(|e| anyhow::anyhow!(e.0))?;
    Ok(StoreRead {
        vitals: parse_xcode_metrics(&metrics),
        issues: Vec::new(),
        asc_app_id: Some(app_id),
    })
}

/// Picks hang rate and launch time from Apple's Xcode metrics document: the
/// iOS product, the all-iPhones dataset where there is one, the typical
/// (50th percentile) user, one point per app version.
fn parse_xcode_metrics(document: &Value) -> Vec<StoreVital> {
    let Some(products) = document["productData"].as_array() else {
        return Vec::new();
    };
    let Some(product) = products
        .iter()
        .find(|product| {
            product["platform"]
                .as_str()
                .is_some_and(|platform| platform.eq_ignore_ascii_case("ios"))
        })
        .or_else(|| products.first())
    else {
        return Vec::new();
    };
    let categories = product["metricCategories"].as_array();
    [
        (StoreVitalKind::HangRate, "HANG", "hang"),
        (StoreVitalKind::LaunchTime, "LAUNCH", "launch"),
    ]
    .into_iter()
    .filter_map(|(kind, category, metric_hint)| {
        let metrics = categories?
            .iter()
            .find(|entry| entry["identifier"] == category)?["metrics"]
            .as_array()?;
        let metric = metrics
            .iter()
            .find(|metric| {
                metric["identifier"]
                    .as_str()
                    .is_some_and(|id| id.to_lowercase().contains(metric_hint))
            })
            .or_else(|| metrics.first())?;
        let datasets = metric["datasets"].as_array()?;
        let criteria = |dataset: &Value, key: &str| {
            dataset["filterCriteria"][key]
                .as_str()
                .unwrap_or_default()
                .to_lowercase()
        };
        let dataset = datasets.iter().max_by_key(|dataset| {
            let device = criteria(dataset, "device");
            let percentile = criteria(dataset, "percentile");
            (device.contains("all") as u8) * 2
                + (percentile.contains("50") || percentile.contains("typical")) as u8
        })?;
        let points: Vec<StoreVitalPoint> = dataset["points"]
            .as_array()
            .map(|points| {
                points
                    .iter()
                    .map(|point| StoreVitalPoint {
                        label: point["version"].as_str().unwrap_or_default().to_string(),
                        value: point["value"].as_f64(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(StoreVital {
            kind,
            value: points.iter().rev().find_map(|point| point.value),
            unit: metric["unit"]["displayName"]
                .as_str()
                .or_else(|| metric["unit"]["identifier"].as_str())
                .unwrap_or_default()
                .to_string(),
            threshold: None,
            points,
        })
    })
    .collect()
}

pub fn start_store_vitals_monitor(state: Arc<AppState>) {
    tokio::spawn(async move {
        let http = match reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
        {
            Ok(http) => http,
            Err(error) => {
                error!(%error, "store vitals monitor could not build its HTTP client");
                return;
            }
        };
        loop {
            if let Err(error) = discover_apps(&state.db).await {
                error!(%error, "store vitals: failed to discover apps");
            } else if let Err(error) = refresh_due(&state, &http).await {
                error!(%error, "store vitals: failed to record a store read");
            }
            MONITOR_IDLE.notify_waiters();
            tokio::select! {
                _ = tokio::time::sleep(MONITOR_INTERVAL) => {}
                _ = MONITOR_WAKE.notified() => {}
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        for statement in [
            "INSERT INTO users (id, email, oidc_subject, role, created_at, updated_at) \
             VALUES ('u1', 'lead@example.com', 'sub', 'owner', 0, 0)",
            "INSERT INTO projects (id, name, created_by, created_at, updated_at) \
             VALUES ('p1', 'Shop', 'u1', 0, 0)",
            "INSERT INTO pipelines (id, project_id, name, created_at, updated_at) \
             VALUES ('pl1', 'p1', 'android', 0, 0), ('pl2', 'p1', 'android-beta', 0, 0), \
             ('pl3', 'p1', 'ios', 0, 0)",
            "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
             queued_at, created_at, updated_at) VALUES \
             ('b1', 'p1', 'pl1', 1, 'succeeded', 'manual', 0, 0, 0), \
             ('b2', 'p1', 'pl2', 2, 'succeeded', 'manual', 0, 0, 0), \
             ('b3', 'p1', 'pl3', 3, 'succeeded', 'manual', 0, 0, 0)",
            "INSERT INTO releases (id, project_id, build_id, store, track, package_name, state, \
             created_at, updated_at) VALUES \
             ('r1', 'p1', 'b1', 'google_play', 'production', 'com.shop', 'complete', 10, 10), \
             ('r2', 'p1', 'b2', 'google_play', 'beta', 'com.shop', 'rolling', 20, 20), \
             ('r3', 'p1', 'b3', 'app_store', 'app_store', NULL, 'complete', 30, 30)",
            "INSERT INTO app_store_release_links (release_id, pipeline_id, bundle_id, version, \
             build_number, config, asc_app_id, next_poll_at, created_at, updated_at) VALUES \
             ('r3', 'pl3', 'com.shop.ios', '1.0', '1', '{}', '6400000000', 0, 30, 30)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        pool
    }

    #[tokio::test]
    async fn discovers_each_released_app_through_its_newest_release() {
        let pool = pool().await;
        discover_apps(&pool).await.unwrap();
        // Running again keeps one row per app.
        discover_apps(&pool).await.unwrap();

        let rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT store, app_identifier, pipeline_id, asc_app_id FROM store_app_vitals ORDER BY store",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            rows,
            [
                (
                    "app_store".to_string(),
                    "com.shop.ios".to_string(),
                    "pl3".to_string(),
                    Some("6400000000".to_string())
                ),
                (
                    "google_play".to_string(),
                    "com.shop".to_string(),
                    "pl2".to_string(),
                    None
                ),
            ]
        );
    }

    async fn play_apps(pool: &SqlitePool) -> Vec<(String, String)> {
        sqlx::query_as(
            "SELECT app_identifier, pipeline_id FROM store_app_vitals \
             WHERE store = 'google_play' ORDER BY app_identifier",
        )
        .fetch_all(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn discovers_google_play_apps_a_pipeline_is_set_up_for_before_their_first_release() {
        let pool = pool().await;
        for statement in [
            "INSERT INTO pipelines (id, project_id, name, created_at, updated_at) VALUES \
             ('pl4', 'p1', 'wear', 0, 0), ('pl5', 'p1', 'tv', 0, 0), \
             ('pl6', 'p1', 'android-new', 0, 0), ('pl7', 'p1', 'android-unknown', 0, 0)",
            "INSERT INTO pipeline_publish_targets (id, pipeline_id, store, enabled, config, \
             created_at, updated_at) VALUES \
             ('t4', 'pl4', 'google_play', 1, \
              '{\"service_account_secret\":\"PLAY\",\"package_name\":\"com.shop.wear\",\"track\":\"internal\"}', 0, 0), \
             ('t5', 'pl5', 'google_play', 0, \
              '{\"service_account_secret\":\"PLAY\",\"track\":\"internal\"}', 0, 0), \
             ('t6', 'pl6', 'google_play', 1, \
              '{\"service_account_secret\":\"PLAY\",\"package_name\":\"com.shop\",\"track\":\"internal\"}', 0, 50), \
             ('t7', 'pl7', 'google_play', 1, \
              '{\"service_account_secret\":\"PLAY\",\"package_name\":\" \",\"track\":\"internal\"}', 0, 0)",
            "INSERT INTO pipeline_google_play_checks (pipeline_id, service_account_secret, \
             secret_version, ok, message, package_name, checked_at) VALUES \
             ('pl5', 'PLAY', 'v1', 1, 'ok', 'com.shop.tv', 0), \
             ('pl7', 'PLAY', 'v1', 0, 'denied', 'com.shop.denied', 0)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }

        discover_apps(&pool).await.unwrap();
        discover_apps(&pool).await.unwrap();

        // The released app stays with its newest release's pipeline.
        let expected = |rows: &[(&str, &str)]| {
            rows.iter()
                .map(|(app, pipeline)| (app.to_string(), pipeline.to_string()))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            play_apps(&pool).await,
            expected(&[
                ("com.shop", "pl2"),
                ("com.shop.tv", "pl5"),
                ("com.shop.wear", "pl4")
            ])
        );

        // Moving the app to another pipeline follows it; dropping the
        // settings drops an app that was never released.
        sqlx::query(
            "UPDATE pipeline_publish_targets SET config = \
             '{\"service_account_secret\":\"PLAY\",\"package_name\":\"com.shop.tv\",\"track\":\"internal\"}', \
             updated_at = 60 WHERE id = 't6'",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("DELETE FROM pipeline_publish_targets WHERE id = 't4'")
            .execute(&pool)
            .await
            .unwrap();
        discover_apps(&pool).await.unwrap();

        assert_eq!(
            play_apps(&pool).await,
            expected(&[("com.shop", "pl2"), ("com.shop.tv", "pl6")])
        );
    }

    fn read(value: f64) -> anyhow::Result<StoreRead> {
        Ok(StoreRead {
            vitals: vec![play_vital(
                StoreVitalKind::CrashRate,
                PlayVitalsRate {
                    rate_28d: Some(value),
                    days: Vec::new(),
                },
                PLAY_CRASH_THRESHOLD,
            )],
            issues: Vec::new(),
            asc_app_id: None,
        })
    }

    async fn play_row(pool: &SqlitePool) -> (Option<String>, Option<String>, Option<i64>, i64) {
        sqlx::query_as(
            "SELECT vitals_json, error, refreshed_at, next_refresh_at FROM store_app_vitals \
             WHERE store = 'google_play'",
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn a_claimed_row_is_read_once_and_a_refresh_during_the_read_stays_due() {
        let pool = pool().await;
        discover_apps(&pool).await.unwrap();

        let claimed = claim_due(&pool, 100).await.unwrap();
        assert_eq!(claimed.len(), 2);
        assert!(claim_due(&pool, 100).await.unwrap().is_empty());

        // Someone asks for a refresh while the read is running.
        assert_eq!(mark_due(&pool, true, "u1", 110).await.unwrap(), 2);
        let play = claimed
            .iter()
            .find(|row| row.store == "google_play")
            .unwrap();
        record_read(&pool, play, 100, read(0.002)).await.unwrap();

        let (vitals, error, refreshed_at, next_refresh_at) = play_row(&pool).await;
        assert!(vitals.unwrap().contains("0.002"));
        assert_eq!((error, refreshed_at, next_refresh_at), (None, Some(100), 0));
    }

    #[tokio::test]
    async fn an_older_read_never_overwrites_a_newer_one() {
        let pool = pool().await;
        discover_apps(&pool).await.unwrap();
        let slow = claim_due(&pool, 100)
            .await
            .unwrap()
            .into_iter()
            .find(|row| row.store == "google_play")
            .unwrap();
        mark_due(&pool, true, "u1", 150).await.unwrap();
        let fast = claim_due(&pool, 150)
            .await
            .unwrap()
            .into_iter()
            .find(|row| row.store == "google_play")
            .unwrap();

        record_read(&pool, &fast, 150, read(0.002)).await.unwrap();
        record_read(&pool, &slow, 100, Err(anyhow::anyhow!("timed out")))
            .await
            .unwrap();
        record_read(&pool, &slow, 100, read(0.9)).await.unwrap();

        let (vitals, error, refreshed_at, next_refresh_at) = play_row(&pool).await;
        assert!(vitals.unwrap().contains("0.002"));
        assert_eq!(
            (error, refreshed_at, next_refresh_at),
            (None, Some(150), 150 + REFRESH_AFTER_SECS)
        );
    }

    #[tokio::test]
    async fn a_failed_read_keeps_the_last_numbers_and_retries_sooner() {
        let pool = pool().await;
        discover_apps(&pool).await.unwrap();
        let row = |rows: Vec<DueRow>| rows.into_iter().find(|r| r.store == "google_play").unwrap();
        let first = row(claim_due(&pool, 100).await.unwrap());
        record_read(&pool, &first, 100, read(0.002)).await.unwrap();
        mark_due(&pool, true, "u1", 200).await.unwrap();
        let second = row(claim_due(&pool, 200).await.unwrap());

        record_read(
            &pool,
            &second,
            200,
            Err(anyhow::anyhow!("permission denied")),
        )
        .await
        .unwrap();

        let (vitals, error, refreshed_at, next_refresh_at) = play_row(&pool).await;
        assert!(vitals.unwrap().contains("0.002"));
        assert_eq!(
            (error.as_deref(), refreshed_at, next_refresh_at),
            (Some("permission denied"), Some(100), 200 + RETRY_AFTER_SECS)
        );
    }

    #[tokio::test]
    async fn manual_refresh_skips_rows_read_moments_ago() {
        let pool = pool().await;
        discover_apps(&pool).await.unwrap();
        for row in claim_due(&pool, 100).await.unwrap() {
            record_read(&pool, &row, 100, read(0.002)).await.unwrap();
        }

        assert_eq!(mark_due(&pool, true, "u1", 110).await.unwrap(), 0);
        // Not a project member.
        assert_eq!(mark_due(&pool, false, "u1", 200).await.unwrap(), 0);
        assert_eq!(mark_due(&pool, true, "u1", 200).await.unwrap(), 2);
    }

    #[test]
    fn xcode_metrics_pick_the_typical_all_iphones_dataset() {
        let document = serde_json::json!({
            "productData": [{
                "platform": "iOS",
                "metricCategories": [
                    {"identifier": "HANG", "metrics": [{
                        "identifier": "hangRate",
                        "unit": {"identifier": "s/hr", "displayName": "s/hr"},
                        "datasets": [
                            {"filterCriteria": {"device": "iPhone15,2", "percentile": "percentile.fifty"},
                             "points": [{"version": "2.3", "value": 9.0}]},
                            {"filterCriteria": {"device": "all_iPhones", "percentile": "percentile.fifty"},
                             "points": [{"version": "2.3", "value": 4.2}, {"version": "2.4", "value": 3.1}]},
                        ]
                    }]},
                    {"identifier": "LAUNCH", "metrics": [{
                        "identifier": "launchTime",
                        "unit": {"identifier": "ms", "displayName": "ms"},
                        "datasets": [{"filterCriteria": {"device": "all_iPhones", "percentile": "percentile.fifty"},
                                      "points": [{"version": "2.4", "value": 640.0}]}]
                    }]}
                ]
            }]
        });

        let vitals = parse_xcode_metrics(&document);

        assert_eq!(vitals.len(), 2);
        assert_eq!(vitals[0].kind, StoreVitalKind::HangRate);
        assert_eq!(vitals[0].value, Some(3.1));
        assert_eq!(vitals[0].unit, "s/hr");
        assert_eq!(vitals[0].points.len(), 2);
        assert_eq!(vitals[1].kind, StoreVitalKind::LaunchTime);
        assert_eq!(vitals[1].value, Some(640.0));
        assert!(parse_xcode_metrics(&serde_json::json!({})).is_empty());
    }
}
