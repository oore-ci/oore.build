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
use crate::releases::{BoxFuture, ReleaseStoreClient, StoreChange};
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
            // Keep the name, notes and priority the uploader set.
            let mut track_release = current
                .releases
                .into_iter()
                .find(|existing| existing.version_codes.contains(&target.version_code))
                .unwrap_or(TrackRelease {
                    name: None,
                    version_codes: vec![target.version_code],
                    status,
                    user_fraction: None,
                    release_notes: Vec::new(),
                    in_app_update_priority: None,
                });
            track_release.status = status;
            track_release.user_fraction = fraction;
            edits.update_track(&track, &track_release).await
        }
        .await;
        match result {
            Ok(()) => edits.commit().await,
            Err(error) => {
                let _ = edits.delete().await;
                Err(error)
            }
        }
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

impl ReleaseStoreClient for PlayReleaseStoreClient {
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
}
