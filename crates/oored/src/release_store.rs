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
    /// The Play language the pipeline's notes are written in.
    notes_language: String,
}

/// A pipeline's Google Play settings with the service account key they name.
pub(crate) async fn play_service_account(
    pool: &SqlitePool,
    encryption_key: &[u8],
    project_id: &str,
    pipeline_id: &str,
) -> anyhow::Result<(Zeroizing<String>, oore_contract::GooglePlayPublishConfig)> {
    let (config, _) = load_google_play(pool, pipeline_id)
        .await?
        .context("the pipeline has no Google Play settings")?;
    let service_account_json = resolve_secret_value(
        pool,
        encryption_key,
        project_id,
        Some(pipeline_id),
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
    Ok((service_account_json, config))
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
        // An adopted release has no build; it names its pipeline itself.
        let pipeline_id: String = sqlx::query_scalar(
            "SELECT COALESCE(r.pipeline_id, b.pipeline_id) FROM releases r
             LEFT JOIN builds b ON b.id = r.build_id WHERE r.id = ?1",
        )
        .bind(&release.id)
        .fetch_optional(&self.pool)
        .await?
        .flatten()
        .context("the release's build and pipeline no longer exist")?;
        let (service_account_json, config) = play_service_account(
            &self.pool,
            &self.encryption_key,
            &release.project_id,
            &pipeline_id,
        )
        .await?;
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
            notes_language: config.release_notes_locale,
        })
    }

    async fn apply_play(&self, release: &Release, change: StoreChange) -> anyhow::Result<()> {
        if change == StoreChange::Pause {
            return Ok(());
        }
        let target = self.target(release).await?;
        let token =
            oore_google_play::access_token(&self.http, &target.service_account_json).await?;
        let edits = PlayEdits::insert(&self.http, &token, &target.package_name).await?;
        apply_on_edit(
            &edits,
            &oore_google_play::play_track_name(&release.track),
            target.version_code,
            release.rollout_fraction,
            change,
        )
        .await
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
        promote_on_edit(
            &edits,
            &oore_google_play::play_track_name(&source.track),
            &oore_google_play::play_track_name(target.track),
            play.version_code,
            target.fraction,
            target
                .release_notes
                .map(|notes| (notes, play.notes_language.as_str())),
        )
        .await
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

/// Make `change` to the release holding `version_code` on `track` in the
/// open edit, then commit it.
async fn apply_on_edit(
    edits: &PlayEdits<'_>,
    track: &str,
    version_code: i64,
    rollout_fraction: f64,
    change: StoreChange,
) -> anyhow::Result<()> {
    let (status, fraction) = match change {
        StoreChange::Pause => return Ok(()),
        StoreChange::Rollout { fraction } | StoreChange::Resume { fraction } if fraction >= 1.0 => {
            (ReleaseStatus::Completed, None)
        }
        StoreChange::Rollout { fraction } | StoreChange::Resume { fraction } => {
            (ReleaseStatus::InProgress, Some(fraction))
        }
        StoreChange::Halt => (ReleaseStatus::Halted, Some(rollout_fraction)),
        StoreChange::Complete => (ReleaseStatus::Completed, None),
    };
    let staged = async {
        let current = edits.get_track(track).await?;
        // Keep the name, notes and priority the uploader set. A release
        // that is gone was discarded or replaced in Play Console; adding
        // it back would undo that.
        let Some(mut track_release) = current
            .releases
            .into_iter()
            .find(|existing| existing.version_codes.contains(&version_code))
        else {
            bail!(
                "version code {version_code} is no longer on the {track} track in Play Console; it was discarded or replaced there"
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
        edits.update_track(track, &track_release).await
    }
    .await;
    commit_or_discard(edits, staged).await
}

/// Put `version_code` on `to_track` in the open edit, with the name, notes
/// and update priority it has on `from_track`, then commit it.
async fn promote_on_edit(
    edits: &PlayEdits<'_>,
    from_track: &str,
    to_track: &str,
    version_code: i64,
    fraction: Option<f64>,
    // The version's notes and the Play language to send them under.
    notes: Option<(&oore_contract::VersionReleaseNotes, &str)>,
) -> anyhow::Result<()> {
    let staged = async {
        let from = edits.get_track(from_track).await?;
        let template = from
            .releases
            .into_iter()
            .find(|release| release.version_codes.contains(&version_code));
        let (status, user_fraction) = match fraction {
            Some(fraction) if fraction < 1.0 => (ReleaseStatus::InProgress, Some(fraction)),
            _ => (ReleaseStatus::Completed, None),
        };
        let release = TrackRelease {
            name: template.as_ref().and_then(|release| release.name.clone()),
            version_codes: vec![version_code],
            status,
            user_fraction,
            release_notes: match notes {
                Some((notes, language)) => promoted_play_notes(
                    notes,
                    language,
                    template
                        .as_ref()
                        .map(|release| release.release_notes.as_slice()),
                ),
                None => template
                    .as_ref()
                    .map_or_default(|release| release.release_notes.clone()),
            },
            in_app_update_priority: template.and_then(|release| release.in_app_update_priority),
        };
        edits.update_track(to_track, &release).await
    }
    .await;
    commit_or_discard(edits, staged).await
}

/// Commit the edit once its changes are staged. When staging or the commit
/// fails, delete the edit: an open edit would get in the way of the next one.
async fn commit_or_discard(
    edits: &PlayEdits<'_>,
    staged: anyhow::Result<()>,
) -> anyhow::Result<()> {
    let result = match staged {
        Ok(()) => edits.commit().await.map(|outcome| {
            if outcome == oore_google_play::CommitOutcome::HeldForReview {
                tracing::warn!("Play Console holds this change until someone sends it for review");
            }
        }),
        Err(error) => Err(error),
    };
    if result.is_err() {
        let _ = edits.delete().await;
    }
    result
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

    /// Serves `replies` (status, body) in turn, one per connection. Returns
    /// the request lines.
    async fn mock_play(
        replies: Vec<(u16, &'static str)>,
    ) -> (String, tokio::task::JoinHandle<Vec<String>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let mut seen = Vec::new();
            for (status, body) in replies {
                let Ok(Ok((mut socket, _))) =
                    tokio::time::timeout(Duration::from_secs(2), listener.accept()).await
                else {
                    break;
                };
                let mut buf = vec![0u8; 16 * 1024];
                let read = socket.read(&mut buf).await.unwrap();
                let head = String::from_utf8_lossy(&buf[..read]).to_string();
                seen.push(head.lines().next().unwrap_or_default().to_string());
                let response = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
            seen
        });
        (base, server)
    }

    const OPEN_EDIT: (u16, &str) = (200, r#"{"id":"e1"}"#);
    const PRODUCTION: (u16, &str) = (
        200,
        r#"{"track":"production","releases":[{"versionCodes":["22"],"status":"inProgress","userFraction":0.1}]}"#,
    );
    const INTERNAL: (u16, &str) = (
        200,
        r#"{"track":"internal","releases":[{"name":"1.2.0","versionCodes":["22"],"status":"completed"}]}"#,
    );
    const REFUSED: (u16, &str) = (
        400,
        r#"{"error":{"code":400,"message":"Track type does not support staged rollouts."}}"#,
    );
    const DELETED: (u16, &str) = (204, "");
    const UPDATE: &str = "PUT /com.example/edits/e1/tracks/";
    const COMMIT: &str = "POST /com.example/edits/e1:commit ";
    const DELETE: &str = "DELETE /com.example/edits/e1 ";

    async fn open_edit<'a>(
        http: &'a reqwest::Client,
        replies: Vec<(u16, &'static str)>,
    ) -> (PlayEdits<'a>, tokio::task::JoinHandle<Vec<String>>) {
        let (base, server) = mock_play(replies).await;
        let edits = PlayEdits::insert_at(http, &base, &base, "token", "com.example")
            .await
            .unwrap();
        (edits, server)
    }

    #[tokio::test]
    async fn a_refused_promotion_deletes_the_edit() {
        let http = reqwest::Client::new();
        let (edits, server) = open_edit(&http, vec![OPEN_EDIT, INTERNAL, REFUSED, DELETED]).await;
        let error = promote_on_edit(&edits, "internal", "alpha", 22, Some(0.1), None)
            .await
            .unwrap_err();
        assert!(
            format!("{error:#}").contains("staged rollouts"),
            "{error:#}"
        );
        let seen = server.await.unwrap();
        assert!(seen[2].starts_with(&format!("{UPDATE}alpha ")), "{seen:?}");
        assert!(seen[3].starts_with(DELETE), "{seen:?}");
        assert!(!seen.iter().any(|line| line.starts_with(COMMIT)));
    }

    #[tokio::test]
    async fn a_promotion_commits_and_keeps_the_edit() {
        let http = reqwest::Client::new();
        let (edits, server) =
            open_edit(&http, vec![OPEN_EDIT, INTERNAL, (200, "{}"), (200, "{}")]).await;
        promote_on_edit(&edits, "internal", "production", 22, Some(0.1), None)
            .await
            .unwrap();
        let seen = server.await.unwrap();
        assert!(seen[3].starts_with(COMMIT), "{seen:?}");
        assert!(!seen.iter().any(|line| line.starts_with(DELETE)));
    }

    #[tokio::test]
    async fn a_refused_rollout_change_deletes_the_edit() {
        let http = reqwest::Client::new();
        let (edits, server) = open_edit(&http, vec![OPEN_EDIT, PRODUCTION, REFUSED, DELETED]).await;
        apply_on_edit(
            &edits,
            "production",
            22,
            0.1,
            StoreChange::Rollout { fraction: 0.2 },
        )
        .await
        .unwrap_err();
        let seen = server.await.unwrap();
        assert!(
            seen[2].starts_with(&format!("{UPDATE}production ")),
            "{seen:?}"
        );
        assert!(seen[3].starts_with(DELETE), "{seen:?}");
        assert!(!seen.iter().any(|line| line.starts_with(COMMIT)));
    }

    #[tokio::test]
    async fn a_release_gone_from_play_deletes_the_edit_without_changing_it() {
        let http = reqwest::Client::new();
        let (edits, server) = open_edit(&http, vec![OPEN_EDIT, PRODUCTION, DELETED]).await;
        let error = apply_on_edit(&edits, "production", 99, 0.1, StoreChange::Complete)
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("no longer on the production track")
        );
        let seen = server.await.unwrap();
        assert_eq!(seen.len(), 3, "{seen:?}");
        assert!(seen[2].starts_with(DELETE), "{seen:?}");
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

/// Play's longest release notes per language.
const PLAY_NOTES_CHARS: usize = 500;

/// The version's notes for a Play promotion: its default text in the
/// pipeline's notes language, replacing that language's notes on the source
/// track and keeping the others.
fn promoted_play_notes(
    notes: &oore_contract::VersionReleaseNotes,
    language: &str,
    template: Option<&[oore_google_play::LocalizedText]>,
) -> Vec<oore_google_play::LocalizedText> {
    let mut out: Vec<oore_google_play::LocalizedText> = template
        .unwrap_or_default()
        .iter()
        .filter(|note| note.language != language)
        .cloned()
        .collect();
    let text: String = notes.text.trim().chars().take(PLAY_NOTES_CHARS).collect();
    if !text.is_empty() {
        out.insert(
            0,
            oore_google_play::LocalizedText {
                language: language.to_string(),
                text,
            },
        );
    }
    out
}

#[cfg(test)]
mod promoted_notes_tests {
    use super::*;

    #[test]
    fn the_version_notes_replace_their_language_only() {
        let notes = oore_contract::VersionReleaseNotes {
            project_id: "p".into(),
            version_name: "2.5.1".into(),
            text: "Faster sync".into(),
            locales: Vec::new(),
            source: oore_contract::ReleaseNotesSource::Manual,
            source_path: None,
            drafted_since: None,
            updated_at: 0,
            updated_by: None,
        };
        let template = vec![
            oore_google_play::LocalizedText {
                language: "en-US".into(),
                text: "Old".into(),
            },
            oore_google_play::LocalizedText {
                language: "de-DE".into(),
                text: "Alt".into(),
            },
        ];
        let out = promoted_play_notes(&notes, "en-US", Some(&template));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].text, "Faster sync");
        assert_eq!(out[1].language, "de-DE");
    }
}
