//! Releases made outside Oore: a release staged in Play Console or a version
//! submitted in App Store Connect shows on Home next to Oore's own, with the
//! same controls.
//!
//! The sync reads every app Oore knows from its store vitals registry (the
//! apps its pipelines publish) and records each store release Oore has no
//! row for as an `adopted` release. From there an adopted release is
//! followed like any other: the rollout monitor syncs Play releases, the App
//! Store monitor follows App Store versions through review. Adopted Play
//! rollouts start without a schedule (`auto_rollout` off); attaching one
//! hands their steps to Oore.

use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use oore_contract::{
    ApiError, AppStoreDestination, AppStorePublishConfig, ReleaseOrigin, ReleasePolicy,
    ReleaseState, ReleaseStore, StoreSyncRequest, StoreSyncResponse,
};
use oore_google_play::{PlayEdits, ReleaseStatus, TrackState};
use sqlx::{FromRow, SqlitePool};
use tracing::{error, info, warn};

use crate::AppState;
use crate::app_store_connect::{AppStoreConnect, AscListedVersion};
use crate::app_store_publish;
use crate::extractors::AuthUser;
use crate::project_rbac::{
    ProjectPermission, check_project_permission, resolve_effective_project_role,
};
use crate::release_store::play_service_account;
use crate::releases::{self, NewRelease, ReleaseActor, ReleaseDeps};
use crate::util::{api_err, now_unix};

/// How often the background sync reads the stores.
const SYNC_INTERVAL: Duration = Duration::from_secs(15 * 60);
/// App Store versions read per app; older ones are history.
const ASC_VERSIONS: u32 = 5;

#[derive(Debug, Clone, FromRow)]
struct KnownApp {
    project_id: String,
    store: String,
    app_identifier: String,
    pipeline_id: String,
    asc_app_id: Option<String>,
}

/// A store release that is not yet a release row.
#[derive(Debug, Clone, PartialEq)]
struct Found {
    track: String,
    version_name: Option<String>,
    version_code: Option<i64>,
    state: ReleaseState,
    fraction: f64,
    reason: String,
    /// App Store only.
    asc_version_id: Option<String>,
    asc_build_id: Option<String>,
    build_number: Option<String>,
    store_status: Option<String>,
}

/// What on Play's tracks Oore should follow: every rollout, halt and draft,
/// and the newest completed release per track.
fn play_found(tracks: &[TrackState]) -> Vec<Found> {
    let mut found = Vec::new();
    for track in tracks {
        let mut newest_completed: Option<i64> = None;
        for release in &track.releases {
            let Some(code) = release.version_codes.iter().copied().max() else {
                continue;
            };
            let (state, fraction, reason) = match release.status {
                ReleaseStatus::InProgress => (
                    ReleaseState::Rolling,
                    release.user_fraction.unwrap_or(0.0),
                    "Found rolling out in Play Console",
                ),
                ReleaseStatus::Halted => (
                    ReleaseState::Halted,
                    release.user_fraction.unwrap_or(0.0),
                    "Found halted in Play Console",
                ),
                ReleaseStatus::Draft => (
                    ReleaseState::Paused,
                    0.0,
                    "Found as a draft in Play Console",
                ),
                ReleaseStatus::Completed => {
                    newest_completed = Some(newest_completed.map_or(code, |c| c.max(code)));
                    continue;
                }
            };
            found.push(Found {
                track: track.track.clone(),
                version_name: release.name.clone(),
                version_code: Some(code),
                state,
                fraction,
                reason: reason.to_string(),
                asc_version_id: None,
                asc_build_id: None,
                build_number: None,
                store_status: None,
            });
        }
        if let Some(code) = newest_completed
            && !found
                .iter()
                .any(|f| f.track == track.track && f.version_code.is_some_and(|c| c > code))
        {
            let name = track
                .releases
                .iter()
                .find(|release| release.version_codes.contains(&code))
                .and_then(|release| release.name.clone());
            found.push(Found {
                track: track.track.clone(),
                version_name: name,
                version_code: Some(code),
                state: ReleaseState::Complete,
                fraction: 1.0,
                reason: "Found live in Play Console".to_string(),
                asc_version_id: None,
                asc_build_id: None,
                build_number: None,
                store_status: None,
            });
        }
    }
    found
}

/// What in App Store Connect Oore should follow: every version on its way
/// to the store, and the newest live one.
fn app_store_found(versions: &[AscListedVersion]) -> Vec<Found> {
    let mut found = Vec::new();
    let mut live_seen = false;
    for version in versions {
        let outcome = app_store_publish::review_outcome(&version.state, &version.version_string);
        let (state, fraction, reason) = match outcome {
            None => {
                if live_seen {
                    continue;
                }
                live_seen = true;
                (
                    ReleaseState::Complete,
                    1.0,
                    "Found live on the App Store".to_string(),
                )
            }
            // History, not a release to follow.
            Some((ReleaseState::Complete | ReleaseState::Halted, _)) => continue,
            Some((state, reason)) => (state, 0.0, reason),
        };
        found.push(Found {
            track: "app_store".to_string(),
            version_name: Some(version.version_string.clone()),
            version_code: version
                .build_number
                .as_deref()
                .and_then(|number| number.split('.').next()?.parse().ok()),
            state,
            fraction,
            reason,
            asc_version_id: Some(version.id.clone()),
            asc_build_id: version.build_id.clone(),
            build_number: version.build_number.clone(),
            store_status: Some(version.state.clone()),
        });
    }
    found
}

/// Whether a release row already covers this store release.
async fn already_known(pool: &SqlitePool, app: &KnownApp, found: &Found) -> sqlx::Result<bool> {
    let known: Option<i64> = if app.store == "app_store" {
        sqlx::query_scalar(
            "SELECT 1 FROM releases r
             LEFT JOIN app_store_release_links l ON l.release_id = r.id
             WHERE r.project_id = ?1 AND r.store = 'app_store' AND r.package_name = ?2
               AND r.track = 'app_store'
               AND (l.asc_version_id = ?3 OR r.version_name = ?4)
               AND r.state <> 'failed'
             LIMIT 1",
        )
        .bind(&app.project_id)
        .bind(&app.app_identifier)
        .bind(&found.asc_version_id)
        .bind(&found.version_name)
        .fetch_optional(pool)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT 1 FROM releases
             WHERE project_id = ?1 AND store = 'google_play' AND package_name = ?2
               AND version_code >= ?3 AND track = ?4
             LIMIT 1",
        )
        .bind(&app.project_id)
        .bind(&app.app_identifier)
        .bind(found.version_code)
        .bind(&found.track)
        .fetch_optional(pool)
        .await?
    };
    Ok(known.is_some())
}

async fn adopt(deps: ReleaseDeps<'_>, app: &KnownApp, found: Found) -> anyhow::Result<()> {
    let store = if app.store == "app_store" {
        ReleaseStore::AppStore
    } else {
        ReleaseStore::GooglePlay
    };
    let mut tx = deps.pool.begin_with("BEGIN IMMEDIATE").await?;
    let created = releases::insert_release(
        &mut tx,
        NewRelease {
            project_id: app.project_id.clone(),
            build_id: None,
            artifact_id: None,
            store,
            track: found.track.clone(),
            package_name: Some(app.app_identifier.clone()),
            version_name: found.version_name.clone(),
            version_code: found.version_code,
            state: found.state,
            rollout_fraction: found.fraction,
            policy: ReleasePolicy::default(),
            environment_id: None,
            promoted_from_release_id: None,
            origin: ReleaseOrigin::Adopted,
            pipeline_id: Some(app.pipeline_id.clone()),
            // Oore only steps a found rollout once someone attaches a schedule.
            auto_rollout: false,
        },
        &ReleaseActor::System,
        Some(&found.reason),
    )
    .await
    .map_err(|error| anyhow::anyhow!("failed to record a store release: {error:?}"))?;
    if store == ReleaseStore::AppStore {
        let config = serde_json::to_string(&AppStorePublishConfig {
            enabled: true,
            destination: AppStoreDestination::AppStore,
            ..AppStorePublishConfig::default()
        })?;
        let stage = if found.state == ReleaseState::Complete {
            "live"
        } else {
            "review"
        };
        let now = now_unix();
        sqlx::query(
            "INSERT INTO app_store_release_links (
                release_id, pipeline_id, bundle_id, version, build_number, config, stage,
                asc_app_id, asc_build_id, asc_version_id, store_status, next_poll_at,
                created_at, updated_at, remapped_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12, ?12, ?12)",
        )
        .bind(&created.release.id)
        .bind(&app.pipeline_id)
        .bind(&app.app_identifier)
        .bind(found.version_name.as_deref().unwrap_or_default())
        .bind(found.build_number.as_deref().unwrap_or_default())
        .bind(config)
        .bind(stage)
        .bind(&app.asc_app_id)
        .bind(&found.asc_build_id)
        .bind(&found.asc_version_id)
        .bind(&found.store_status)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    let release = releases::finish_create(deps, created, &ReleaseActor::System).await;
    info!(
        release_id = %release.id,
        store = %release.store,
        track = %release.track,
        "adopted a release made outside Oore"
    );
    Ok(())
}

async fn read_play(
    state: &AppState,
    http: &reqwest::Client,
    app: &KnownApp,
) -> anyhow::Result<Vec<Found>> {
    let (service_account, _) = play_service_account(
        &state.db,
        &state.encryption_key,
        &app.project_id,
        &app.pipeline_id,
    )
    .await?;
    let token = oore_google_play::access_token(http, &service_account).await?;
    let edits = PlayEdits::insert(http, &token, &app.app_identifier).await?;
    let tracks = edits.list_tracks().await;
    if let Err(error) = edits.delete().await {
        warn!(%error, "failed to discard the Google Play read edit");
    }
    Ok(play_found(&tracks?))
}

async fn read_app_store(
    state: &AppState,
    http: &reqwest::Client,
    app: &mut KnownApp,
) -> anyhow::Result<Vec<Found>> {
    let creds = app_store_publish::load_credentials(state, &app.pipeline_id).await?;
    let asc = AppStoreConnect::new(http, &creds);
    let app_id = match &app.asc_app_id {
        Some(id) => id.clone(),
        None => {
            let id = asc
                .find_app_id(&app.app_identifier)
                .await
                .map_err(|error| anyhow::anyhow!(error.0))?
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "no App Store Connect app has bundle ID {}",
                        app.app_identifier
                    )
                })?;
            app.asc_app_id = Some(id.clone());
            id
        }
    };
    let versions = asc
        .list_versions(&app_id, ASC_VERSIONS)
        .await
        .map_err(|error| anyhow::anyhow!(error.0))?;
    Ok(app_store_found(&versions))
}

/// Reads the stores for the known apps, of one project or all, and adopts
/// the releases Oore has no row for.
pub(crate) async fn sync(
    state: &AppState,
    http: &reqwest::Client,
    project_ids: Option<&[String]>,
) -> anyhow::Result<StoreSyncResponse> {
    let apps: Vec<KnownApp> = sqlx::query_as(
        "SELECT project_id, store, app_identifier, pipeline_id, asc_app_id FROM store_app_vitals
         ORDER BY project_id, store, app_identifier",
    )
    .fetch_all(&state.db)
    .await?;
    let deps = ReleaseDeps::from(state);
    let mut response = StoreSyncResponse::default();
    for mut app in apps {
        if project_ids.is_some_and(|ids| !ids.contains(&app.project_id)) {
            continue;
        }
        response.apps += 1;
        let read = if app.store == "app_store" {
            read_app_store(state, http, &mut app).await
        } else {
            read_play(state, http, &app).await
        };
        let found = match read {
            Ok(found) => found,
            Err(error) => {
                response
                    .errors
                    .push(format!("{} ({}): {error:#}", app.app_identifier, app.store));
                continue;
            }
        };
        for found in found {
            if already_known(&state.db, &app, &found).await? {
                continue;
            }
            match adopt(deps, &app, found).await {
                Ok(()) => response.adopted += 1,
                Err(error) => response
                    .errors
                    .push(format!("{} ({}): {error:#}", app.app_identifier, app.store)),
            }
        }
    }
    Ok(response)
}

fn sync_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()
        .expect("failed to build the store sync HTTP client")
}

/// `POST /v1/releases/store-sync`: read the stores now. Maintainers of a
/// project may sync it; instance admins may sync every project.
pub async fn sync_store_releases(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(request): Json<StoreSyncRequest>,
) -> Result<Json<StoreSyncResponse>, (StatusCode, Json<ApiError>)> {
    let project_ids: Vec<String> = match &request.project_id {
        Some(project_id) => vec![project_id.clone()],
        None => sqlx::query_scalar("SELECT id FROM projects")
            .fetch_all(&state.db)
            .await
            .map_err(|error| {
                error!(%error, "failed to list projects for store sync");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to sync store releases",
                )
            })?,
    };
    let mut allowed = Vec::new();
    for project_id in project_ids {
        let role = resolve_effective_project_role(
            &state.db,
            &auth.0.user_id,
            &auth.0.role,
            &project_id,
            &auth.0.auth_source,
        )
        .await;
        if role.is_ok_and(|role| check_project_permission(&role, ProjectPermission::ManageReleases))
        {
            allowed.push(project_id);
        }
    }
    if allowed.is_empty() {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "You can't manage releases in that project",
        ));
    }
    sync(&state, &sync_http_client(), Some(&allowed))
        .await
        .map(Json)
        .map_err(|error| {
            error!(%error, "store sync failed");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to sync store releases",
            )
        })
}

pub fn start_store_sync(state: Arc<AppState>) {
    tokio::spawn(async move {
        let http = sync_http_client();
        loop {
            // After the vitals monitor's first discovery of the apps.
            tokio::time::sleep(SYNC_INTERVAL).await;
            match sync(&state, &http, None).await {
                Ok(response) if response.adopted > 0 || !response.errors.is_empty() => {
                    info!(
                        apps = response.apps,
                        adopted = response.adopted,
                        errors = response.errors.len(),
                        "store sync finished"
                    );
                }
                Ok(_) => {}
                Err(error) => error!(%error, "store sync failed"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use oore_google_play::TrackRelease;

    fn release(codes: &[i64], status: ReleaseStatus, fraction: Option<f64>) -> TrackRelease {
        TrackRelease {
            name: Some(format!("v{}", codes[0])),
            version_codes: codes.to_vec(),
            status,
            user_fraction: fraction,
            release_notes: Vec::new(),
            in_app_update_priority: None,
        }
    }

    #[test]
    fn play_adopts_rollouts_and_the_newest_completed_release() {
        let tracks = vec![TrackState {
            track: "production".into(),
            releases: vec![
                release(&[40], ReleaseStatus::Completed, None),
                release(&[42], ReleaseStatus::InProgress, Some(0.2)),
            ],
        }];
        let found = play_found(&tracks);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].state, ReleaseState::Rolling);
        assert_eq!(found[0].version_code, Some(42));
        assert!((found[0].fraction - 0.2).abs() < 1e-9);

        let tracks = vec![TrackState {
            track: "internal".into(),
            releases: vec![
                release(&[30], ReleaseStatus::Completed, None),
                release(&[31], ReleaseStatus::Completed, None),
            ],
        }];
        let found = play_found(&tracks);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].state, ReleaseState::Complete);
        assert_eq!(found[0].version_code, Some(31));
    }

    #[test]
    fn app_store_adopts_versions_in_flight_and_the_newest_live_one() {
        let version = |id: &str, version: &str, state: &str| AscListedVersion {
            id: id.into(),
            version_string: version.into(),
            state: state.into(),
            build_number: Some("12".into()),
            build_id: Some(format!("b-{id}")),
        };
        let found = app_store_found(&[
            version("v3", "1.3.0", "WAITING_FOR_REVIEW"),
            version("v2", "1.2.0", "READY_FOR_SALE"),
            version("v1", "1.1.0", "READY_FOR_SALE"),
            version("v0", "1.0.0", "REPLACED_WITH_NEW_VERSION"),
        ]);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].state, ReleaseState::InReview);
        assert_eq!(found[1].state, ReleaseState::Complete);
        assert_eq!(found[1].version_name.as_deref(), Some("1.2.0"));
        assert_eq!(found[1].version_code, Some(12));
    }
}
