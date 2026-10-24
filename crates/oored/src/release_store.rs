//! Google Play implementation of the release store seam.
//!
//! Calls go out from the daemon host with the pipeline's service account
//! secret, the same credential the runner used for the upload.

use std::time::Duration;

use anyhow::{Context, bail};
use oore_contract::{Release, ReleaseStore};
use oore_google_play::{PlayEdits, ReleaseStatus, TrackRelease};
use sqlx::SqlitePool;
use zeroize::Zeroizing;

use crate::pipeline_publish::load_google_play;
use crate::releases::{
    BoxFuture, Promoted, PromotionTarget, ReleaseStoreClient, StoreChange, StoreObservation,
};
use crate::secrets::resolve_secret_value;

pub struct PlayReleaseStoreClient {
    pool: SqlitePool,
    encryption_key: Zeroizing<Vec<u8>>,
    http: reqwest::Client,
}

struct PlayTarget {
    service_account_json: Zeroizing<String>,
    package_name: String,
    version_code: i64,
}

impl PlayReleaseStoreClient {
    pub fn new(pool: SqlitePool, encryption_key: Zeroizing<Vec<u8>>) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .context("failed to build the Google Play HTTP client")?;
        Ok(Self {
            pool,
            encryption_key,
            http,
        })
    }

    async fn target(&self, release: &Release) -> anyhow::Result<PlayTarget> {
        let pipeline_id: String =
            sqlx::query_scalar("SELECT pipeline_id FROM builds WHERE id = ?1")
                .bind(&release.build_id)
                .fetch_optional(&self.pool)
                .await?
                .context("the release's build no longer exists")?;
        let (config, _) = load_google_play(&self.pool, &pipeline_id)
            .await?
            .context("the pipeline has no Google Play settings")?;
        let service_account_json = resolve_secret_value(
            &self.pool,
            &self.encryption_key,
            &release.project_id,
            Some(&pipeline_id),
            &config.service_account_secret,
        )
        .await?
        .map(Zeroizing::new)
        .with_context(|| {
            format!(
                "the service account secret {} is not set",
                config.service_account_secret
            )
        })?;
        let package_name = release
            .package_name
            .clone()
            .or(config.package_name)
            .context("the release has no package name")?;
        let version_code = release
            .version_code
            .context("the release has no version code")?;
        Ok(PlayTarget {
            service_account_json,
            package_name,
            version_code,
        })
    }

    async fn apply_play(&self, release: &Release, change: StoreChange) -> anyhow::Result<()> {
        let (status, fraction) = match change {
            StoreChange::Pause => return Ok(()),
            StoreChange::Rollout { fraction } | StoreChange::Resume { fraction }
                if fraction >= 1.0 =>
            {
                (ReleaseStatus::Completed, None)
            }
            StoreChange::Rollout { fraction } | StoreChange::Resume { fraction } => {
                (ReleaseStatus::InProgress, Some(fraction))
            }
            StoreChange::Halt => (ReleaseStatus::Halted, Some(release.rollout_fraction)),
            StoreChange::Complete => (ReleaseStatus::Completed, None),
        };
        let target = self.target(release).await?;
        let token =
            oore_google_play::access_token(&self.http, &target.service_account_json).await?;
        let edits = PlayEdits::insert(&self.http, &token, &target.package_name).await?;
        let track = oore_google_play::play_track_name(&release.track);
        let result = async {
            let current = edits.get_track(&track).await?;
            // Keep the name, notes and priority the uploader set. A release
            // that is gone was discarded or replaced in Play Console; adding
            // it back would undo that.
            let Some(mut track_release) = current
                .releases
                .into_iter()
                .find(|existing| existing.version_codes.contains(&target.version_code))
            else {
                bail!(
                    "version code {} is no longer on the {track} track in Play Console; it was discarded or replaced there",
                    target.version_code
                );
            };
            // Play is the source of truth. A change planned against an older
            // view, such as a scheduled step racing a halt, must not
            // overwrite what Play holds now.
            if !play_allows(change, track_release.status) {
                bail!(
                    "Play Console shows this release as {} now, so it was not changed; reload and try again",
                    play_status_label(track_release.status)
                );
            }
            track_release.status = status;
            track_release.user_fraction = fraction;
            edits.update_track(&track, &track_release).await
        }
        .await;
        let result = match result {
            Ok(()) => edits.commit().await,
            Err(error) => Err(error),
        };
        // A failed commit leaves the edit open, and an open edit would get
        // in the way of the next one.
        if result.is_err() {
            let _ = edits.delete().await;
        }
        result
    }

    /// Assign the source release's version code to another track in one
    /// edit, keeping the release name, notes and update priority it had.
    async fn promote_play(
        &self,
        source: &Release,
        target: PromotionTarget<'_>,
    ) -> anyhow::Result<()> {
        let play = self.target(source).await?;
        let token = oore_google_play::access_token(&self.http, &play.service_account_json).await?;
        let edits = PlayEdits::insert(&self.http, &token, &play.package_name).await?;
        let to_track = oore_google_play::play_track_name(target.track);
        let result = async {
            let from = edits
                .get_track(&oore_google_play::play_track_name(&source.track))
                .await?;
            let template = from
                .releases
                .into_iter()
                .find(|release| release.version_codes.contains(&play.version_code));
            let (status, user_fraction) = match target.fraction {
                Some(fraction) if fraction < 1.0 => (ReleaseStatus::InProgress, Some(fraction)),
                _ => (ReleaseStatus::Completed, None),
            };
            let release = TrackRelease {
                name: template.as_ref().and_then(|release| release.name.clone()),
                version_codes: vec![play.version_code],
                status,
                user_fraction,
                release_notes: template
                    .as_ref()
                    .map(|release| release.release_notes.clone())
                    .unwrap_or_default(),
                in_app_update_priority: template.and_then(|release| release.in_app_update_priority),
            };
            edits.update_track(&to_track, &release).await
        }
        .await;
        let result = match result {
            Ok(()) => edits.commit().await,
            Err(error) => Err(error),
        };
        if result.is_err() {
            let _ = edits.delete().await;
        }
        result
    }

    async fn observe_play(&self, release: &Release) -> anyhow::Result<StoreObservation> {
        let target = self.target(release).await?;
        let token =
            oore_google_play::access_token(&self.http, &target.service_account_json).await?;
        // A read-only edit, deleted afterwards and never committed.
        let edits = PlayEdits::insert(&self.http, &token, &target.package_name).await?;
        let track = edits
            .get_track(&oore_google_play::play_track_name(&release.track))
            .await;
        let _ = edits.delete().await;
        Ok(observation(&track?.releases, target.version_code))
    }

    async fn play_crash_rate(&self, release: &Release) -> anyhow::Result<Option<f64>> {
        let target = self.target(release).await?;
        let token = oore_google_play::access_token_with_scope(
            &self.http,
            &target.service_account_json,
            oore_google_play::REPORTING_SCOPE,
        )
        .await?;
        oore_google_play::crash_rate(
            &self.http,
            &token,
            &target.package_name,
            target.version_code,
        )
        .await
    }
}

/// Whether `change` makes sense for a release Play currently shows as `current`.
fn play_allows(change: StoreChange, current: ReleaseStatus) -> bool {
    use ReleaseStatus::{Completed, Draft, Halted, InProgress};
    match change {
        // A step only raises a rollout that is still in progress in Play.
        StoreChange::Rollout { .. } => current == InProgress,
        StoreChange::Resume { .. } => matches!(current, InProgress | Halted | Draft),
        StoreChange::Halt => matches!(current, InProgress | Halted),
        StoreChange::Complete => matches!(current, InProgress | Draft),
        StoreChange::Pause => current != Completed,
    }
}

fn play_status_label(status: ReleaseStatus) -> &'static str {
    match status {
        ReleaseStatus::Completed => "completed",
        ReleaseStatus::Halted => "halted",
        ReleaseStatus::InProgress => "rolling out",
        ReleaseStatus::Draft => "a draft",
    }
}

/// Where the release holding `version_code` stands among a track's releases.
fn observation(releases: &[TrackRelease], version_code: i64) -> StoreObservation {
    match releases
        .iter()
        .find(|release| release.version_codes.contains(&version_code))
    {
        None => StoreObservation::Missing,
        Some(release) => match release.status {
            ReleaseStatus::Completed => StoreObservation::Completed,
            ReleaseStatus::Halted => StoreObservation::Halted,
            ReleaseStatus::InProgress => StoreObservation::Rolling {
                fraction: release.user_fraction.unwrap_or(0.0),
            },
            ReleaseStatus::Draft => StoreObservation::Other,
        },
    }
}

impl ReleaseStoreClient for PlayReleaseStoreClient {
    fn observe<'a>(
        &'a self,
        release: &'a Release,
    ) -> BoxFuture<'a, anyhow::Result<Option<StoreObservation>>> {
        Box::pin(async move {
            match release.store {
                ReleaseStore::GooglePlay => self.observe_play(release).await.map(Some),
                ReleaseStore::AppStore => Ok(None),
            }
        })
    }

    fn apply<'a>(
        &'a self,
        release: &'a Release,
        change: StoreChange,
    ) -> BoxFuture<'a, anyhow::Result<()>> {
        Box::pin(async move {
            match release.store {
                ReleaseStore::GooglePlay => self.apply_play(release, change).await,
                // App Store phased release is driven by the App Store flow.
                ReleaseStore::AppStore => Ok(()),
            }
        })
    }

    fn crash_rate<'a>(
        &'a self,
        release: &'a Release,
    ) -> BoxFuture<'a, anyhow::Result<Option<f64>>> {
        Box::pin(async move {
            if release.store != ReleaseStore::GooglePlay {
                bail!("crash rate is only read for Google Play releases");
            }
            self.play_crash_rate(release).await
        })
    }

    fn promote<'a>(
        &'a self,
        source: &'a Release,
        target: PromotionTarget<'a>,
    ) -> BoxFuture<'a, anyhow::Result<Promoted>> {
        Box::pin(async move {
            if source.store != ReleaseStore::GooglePlay {
                bail!("only Google Play releases can be promoted here");
            }
            self.promote_play(source, target).await?;
            Ok(Promoted::default())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(status: ReleaseStatus, codes: &[i64], fraction: Option<f64>) -> TrackRelease {
        TrackRelease {
            name: None,
            version_codes: codes.to_vec(),
            status,
            user_fraction: fraction,
            release_notes: Vec::new(),
            in_app_update_priority: None,
        }
    }

    #[test]
    fn play_changes_never_overwrite_a_newer_play_state() {
        let step = StoreChange::Rollout { fraction: 0.2 };
        assert!(play_allows(step, ReleaseStatus::InProgress));
        // A scheduled step that lost a race with a halt must not resume it.
        assert!(!play_allows(step, ReleaseStatus::Halted));
        assert!(!play_allows(step, ReleaseStatus::Completed));
        assert!(play_allows(
            StoreChange::Resume { fraction: 0.05 },
            ReleaseStatus::Halted
        ));
        assert!(play_allows(StoreChange::Halt, ReleaseStatus::InProgress));
        assert!(!play_allows(StoreChange::Halt, ReleaseStatus::Completed));
        assert!(!play_allows(StoreChange::Complete, ReleaseStatus::Halted));
    }

    #[test]
    fn observation_finds_the_release_by_version_code() {
        let track = vec![
            release(ReleaseStatus::Completed, &[8], None),
            release(ReleaseStatus::InProgress, &[22], Some(0.1)),
        ];
        assert_eq!(
            observation(&track, 22),
            StoreObservation::Rolling { fraction: 0.1 }
        );
        assert_eq!(observation(&track, 8), StoreObservation::Completed);
        // Discarded in Play Console: only the old release is left.
        assert_eq!(observation(&track[..1], 22), StoreObservation::Missing);
        assert_eq!(
            observation(&[release(ReleaseStatus::Halted, &[22], Some(0.1))], 22),
            StoreObservation::Halted
        );
    }
}
