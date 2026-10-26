//! Store uploads that run on the runner host after a successful build, so
//! store credentials never leave the machine that built the artifact.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use oore_contract::{
    GooglePlayPublishConfig, GooglePlayReleaseNotesSource, PublishStore, RunnerPublishResult,
};
use oore_google_play::{
    LocalizedText, MAX_RELEASE_NOTES_CHARS, PlayEdits, ReleaseStatus, TrackRelease, access_token,
    is_play_rejection, play_track_name, read_aab_manifest,
};

use crate::store_version::{ensure_play_version_code, fetch_play_version_codes};

const BUNDLE_OUTPUT_DIR: &str = "build/app/outputs/bundle";
const MAPPING_OUTPUT_DIR: &str = "build/app/outputs/mapping";

fn collect_files_with_extension(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if file_type.is_dir() {
            collect_files_with_extension(&path, extension, out);
        } else if file_type.is_file() && path.extension().is_some_and(|ext| ext == extension) {
            out.push(path);
        }
    }
}

/// Pick the bundle to upload: the only AAB, or the only release AAB.
fn find_bundle(workspace: &Path) -> anyhow::Result<PathBuf> {
    let mut bundles = Vec::new();
    collect_files_with_extension(&workspace.join(BUNDLE_OUTPUT_DIR), "aab", &mut bundles);
    bundles.sort();
    if bundles.len() > 1 {
        bundles.retain(|path| {
            path.parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .is_some_and(|variant| variant.to_ascii_lowercase().ends_with("release"))
        });
    }
    match bundles.as_slice() {
        [bundle] => Ok(bundle.clone()),
        [] => bail!(
            "No release App Bundle found under {BUNDLE_OUTPUT_DIR}. Build one with `flutter build appbundle`."
        ),
        many => bail!(
            "Found {} release App Bundles ({}); build a single flavor per pipeline to publish",
            many.len(),
            many.iter()
                .filter_map(|path| path.strip_prefix(workspace).ok())
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// `build/app/outputs/mapping/<variant>/mapping.txt` for the bundle's variant.
fn mapping_for_bundle(workspace: &Path, bundle: &Path) -> Option<PathBuf> {
    let variant = bundle.parent()?.file_name()?;
    let mapping = workspace
        .join(MAPPING_OUTPUT_DIR)
        .join(variant)
        .join("mapping.txt");
    mapping.is_file().then_some(mapping)
}

fn truncate_release_notes(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= MAX_RELEASE_NOTES_CHARS {
        return text.to_string();
    }
    let mut truncated: String = text.chars().take(MAX_RELEASE_NOTES_CHARS - 1).collect();
    truncated.push('…');
    truncated
}

fn release_notes(
    workspace: &Path,
    config: &GooglePlayPublishConfig,
    changelog: Option<&str>,
) -> anyhow::Result<Vec<LocalizedText>> {
    match config.release_notes {
        GooglePlayReleaseNotesSource::None => Ok(Vec::new()),
        GooglePlayReleaseNotesSource::Changelog => Ok(changelog
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| {
                vec![LocalizedText {
                    language: config.release_notes_locale.clone(),
                    text: truncate_release_notes(text),
                }]
            })
            .unwrap_or_default()),
        GooglePlayReleaseNotesSource::RepoFiles => {
            let relative = config
                .release_notes_dir
                .as_deref()
                .context("release_notes_dir is not set")?;
            let canonical_workspace = workspace.canonicalize()?;
            let dir = workspace
                .join(relative)
                .canonicalize()
                .with_context(|| format!("release notes directory {relative} does not exist"))?;
            if !dir.starts_with(&canonical_workspace) {
                bail!("release notes directory {relative} resolves outside the workspace");
            }
            let mut notes = Vec::new();
            for entry in fs::read_dir(&dir)? {
                let path = entry?.path();
                if path.extension().is_none_or(|ext| ext != "txt") || !path.is_file() {
                    continue;
                }
                let Some(language) = path.file_stem().and_then(|stem| stem.to_str()) else {
                    continue;
                };
                let text = fs::read_to_string(&path)
                    .with_context(|| format!("failed to read {}", path.display()))?;
                if !text.trim().is_empty() {
                    notes.push(LocalizedText {
                        language: language.to_string(),
                        text: truncate_release_notes(&text),
                    });
                }
            }
            if notes.is_empty() {
                bail!("release notes directory {relative} has no non-empty <locale>.txt files");
            }
            notes.sort_by(|a, b| a.language.cmp(&b.language));
            Ok(notes)
        }
    }
}

/// Status and user fraction for the initial release.
fn release_status(config: &GooglePlayPublishConfig) -> (ReleaseStatus, Option<f64>) {
    if config.draft {
        return (ReleaseStatus::Draft, None);
    }
    match config.rollout_percent {
        Some(percent) if percent < 100.0 => (ReleaseStatus::InProgress, Some(percent / 100.0)),
        _ => (ReleaseStatus::Completed, None),
    }
}

/// Whether the build may still publish: `stopped` ends when it must stop,
/// and `still_active` asks once more right before the release goes live.
pub(crate) struct PublishGate<S, A> {
    pub stopped: S,
    pub still_active: A,
}

/// How a staged edit ended up committed.
#[derive(Debug, PartialEq, Eq)]
enum Committed {
    Confirmed,
    /// The commit's reply was lost, but the release is on the track.
    ReplyLost,
}

/// Runs `stage` on the open edit and commits it unless the build stopped.
/// Every edit that is not committed is deleted, so nothing is left half
/// staged. When a commit's reply is lost, `landed` says whether the release
/// reached the track anyway.
async fn stage_and_commit<T>(
    edits: &PlayEdits<'_>,
    stage: impl Future<Output = anyhow::Result<T>>,
    gate: PublishGate<
        impl Future<Output = anyhow::Error>,
        impl Future<Output = anyhow::Result<()>>,
    >,
    landed: impl Future<Output = anyhow::Result<bool>>,
) -> anyhow::Result<(T, Committed)> {
    let staged = tokio::select! {
        staged = stage => staged,
        error = gate.stopped => Err(error.context("The build stopped during the Google Play upload; nothing was published")),
    };
    let staged = match staged {
        Ok(staged) => staged,
        Err(error) => {
            let _ = edits.delete().await;
            return Err(error);
        }
    };
    if let Err(error) = gate.still_active.await {
        let _ = edits.delete().await;
        return Err(error
            .context("The build stopped before the release was committed; nothing was published"));
    }
    let Err(error) = edits.commit().await else {
        return Ok((staged, Committed::Confirmed));
    };
    if !is_play_rejection(&error) && landed.await.unwrap_or(false) {
        return Ok((staged, Committed::ReplyLost));
    }
    let _ = edits.delete().await;
    Err(error)
}

/// Upload the build's signed App Bundle to Google Play and roll it out.
/// Progress lines go to `log`; they pass through the build's secret masker.
pub(crate) async fn publish_to_google_play(
    http: &reqwest::Client,
    workspace: &Path,
    secrets: &BTreeMap<String, String>,
    changelog: Option<&str>,
    config: &GooglePlayPublishConfig,
    log: &mut Vec<String>,
    gate: PublishGate<
        impl Future<Output = anyhow::Error>,
        impl Future<Output = anyhow::Result<()>>,
    >,
) -> anyhow::Result<RunnerPublishResult> {
    let service_account = secrets
        .get(&config.service_account_secret)
        .with_context(|| {
            format!(
                "Secret '{}' with the Google Play service account JSON is not defined for this pipeline, its project or the instance",
                config.service_account_secret
            )
        })?;

    let bundle = find_bundle(workspace)?;
    let manifest = read_aab_manifest(&bundle)?;
    if let Some(expected) = &config.package_name
        && expected != &manifest.package_name
    {
        bail!(
            "The App Bundle is for {}, but publishing is configured for {expected}",
            manifest.package_name
        );
    }
    let track = play_track_name(&config.track);
    let (status, user_fraction) = release_status(config);
    let notes = release_notes(workspace, config, changelog)?;
    let mapping = config
        .upload_mapping
        .then(|| mapping_for_bundle(workspace, &bundle))
        .flatten();
    log.push(format!(
        "[oore-publish] {} versionCode {} ({}) → track {track}",
        manifest.package_name,
        manifest.version_code,
        manifest.version_name.as_deref().unwrap_or("no versionName"),
    ));

    let token = access_token(http, service_account).await?;
    let store_codes =
        fetch_play_version_codes(http, &token, &manifest.package_name, &track).await?;
    if let Some(highest) = store_codes.track_highest {
        log.push(format!(
            "[oore-publish] Highest versionCode on track {track}: {highest}"
        ));
    }
    ensure_play_version_code(manifest.version_code, &store_codes, &track)?;
    let edits = PlayEdits::insert(http, &token, &manifest.package_name).await?;
    let mut staged_log = Vec::new();
    let stage = async {
        let version_code = edits.upload_bundle(&bundle).await?;
        staged_log.push(format!(
            "[oore-publish] Uploaded {} as versionCode {version_code}",
            bundle.strip_prefix(workspace).unwrap_or(&bundle).display()
        ));
        match &mapping {
            Some(path) => {
                edits.upload_mapping(version_code, path).await?;
                staged_log.push("[oore-publish] Uploaded mapping.txt".to_string());
            }
            None if config.upload_mapping => {
                staged_log.push(
                    "[oore-publish] No mapping.txt found; skipped deobfuscation upload".to_string(),
                );
            }
            None => {}
        }
        edits
            .update_track(
                &track,
                &TrackRelease {
                    name: None,
                    version_codes: vec![version_code],
                    status,
                    user_fraction,
                    release_notes: notes.clone(),
                    in_app_update_priority: config.in_app_update_priority,
                },
            )
            .await?;
        anyhow::Ok(version_code)
    };
    let landed = async {
        let codes = fetch_play_version_codes(http, &token, &manifest.package_name, &track).await?;
        anyhow::Ok(
            codes
                .track_highest
                .is_some_and(|highest| highest >= manifest.version_code),
        )
    };
    let committed = stage_and_commit(&edits, stage, gate, landed).await;
    log.append(&mut staged_log);
    let (version_code, committed) = committed?;
    if committed == Committed::ReplyLost {
        log.push(
            "[oore-publish] Google Play's reply to the commit was lost, but the release is on the track"
                .to_string(),
        );
    }

    log.push(match (status, user_fraction) {
        (ReleaseStatus::Draft, _) => format!("[oore-publish] Created a draft release on {track}"),
        (_, Some(fraction)) => format!(
            "[oore-publish] Rolling out to {}% of {track} users",
            fraction * 100.0
        ),
        _ => format!("[oore-publish] Released to 100% of {track} users"),
    });

    Ok(RunnerPublishResult {
        store: PublishStore::GooglePlay,
        track,
        package_name: manifest.package_name,
        version_code,
        version_name: manifest.version_name,
        rollout_fraction: match status {
            ReleaseStatus::Draft => None,
            _ => Some(user_fraction.unwrap_or(1.0)),
        },
        status: match status {
            ReleaseStatus::Draft => "draft",
            ReleaseStatus::InProgress => "in_progress",
            _ => "completed",
        }
        .to_string(),
        testflight_groups: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> GooglePlayPublishConfig {
        serde_json::from_value(serde_json::json!({
            "enabled": true,
            "service_account_secret": "PLAY_SERVICE_ACCOUNT",
            "track": "production",
        }))
        .unwrap()
    }

    fn workspace(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oore-publish-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(root: &Path, relative: &str, content: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    #[test]
    fn rollout_status_follows_config() {
        let mut config = config();
        assert_eq!(release_status(&config), (ReleaseStatus::Completed, None));
        config.rollout_percent = Some(5.0);
        assert_eq!(
            release_status(&config),
            (ReleaseStatus::InProgress, Some(0.05))
        );
        config.rollout_percent = Some(100.0);
        assert_eq!(release_status(&config), (ReleaseStatus::Completed, None));
        config.draft = true;
        assert_eq!(release_status(&config), (ReleaseStatus::Draft, None));
    }

    #[test]
    fn picks_the_release_bundle_and_its_mapping() {
        let root = workspace("bundle");
        assert!(find_bundle(&root).is_err());
        touch(
            &root,
            "build/app/outputs/bundle/release/app-release.aab",
            "",
        );
        touch(&root, "build/app/outputs/bundle/debug/app-debug.aab", "");
        touch(&root, "build/app/outputs/mapping/release/mapping.txt", "");
        let bundle = find_bundle(&root).unwrap();
        assert!(bundle.ends_with("release/app-release.aab"));
        assert!(mapping_for_bundle(&root, &bundle).is_some());

        touch(&root, "build/app/outputs/bundle/stagingRelease/app.aab", "");
        let error = find_bundle(&root).unwrap_err().to_string();
        assert!(error.contains("Found 2 release App Bundles"), "{error}");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn release_notes_from_changelog_and_repo_files() {
        let root = workspace("notes");
        let mut config = config();
        assert!(release_notes(&root, &config, Some("x")).unwrap().is_empty());

        config.release_notes = GooglePlayReleaseNotesSource::Changelog;
        let long = "a".repeat(600);
        let notes = release_notes(&root, &config, Some(&long)).unwrap();
        assert_eq!(notes[0].language, "en-US");
        assert_eq!(notes[0].text.chars().count(), MAX_RELEASE_NOTES_CHARS);
        assert!(release_notes(&root, &config, None).unwrap().is_empty());

        config.release_notes = GooglePlayReleaseNotesSource::RepoFiles;
        config.release_notes_dir = Some("release-notes".to_string());
        touch(&root, "release-notes/en-US.txt", "Bug fixes\n");
        touch(&root, "release-notes/hi-IN.txt", "सुधार");
        touch(&root, "release-notes/README.md", "ignored");
        let notes = release_notes(&root, &config, None).unwrap();
        assert_eq!(
            notes,
            vec![
                LocalizedText {
                    language: "en-US".to_string(),
                    text: "Bug fixes".to_string()
                },
                LocalizedText {
                    language: "hi-IN".to_string(),
                    text: "सुधार".to_string()
                },
            ]
        );

        config.release_notes_dir = Some("../outside".to_string());
        assert!(release_notes(&root, &config, None).is_err());
        fs::remove_dir_all(&root).ok();
    }

    /// Serves `replies` (status, body) in turn, one per connection; status 0
    /// closes the connection without replying. Returns the request lines.
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
                    tokio::time::timeout(std::time::Duration::from_secs(2), listener.accept())
                        .await
                else {
                    break;
                };
                let mut buf = vec![0u8; 16 * 1024];
                let read = socket.read(&mut buf).await.unwrap();
                let head = String::from_utf8_lossy(&buf[..read]).to_string();
                seen.push(head.lines().next().unwrap_or_default().to_string());
                if status == 0 {
                    continue;
                }
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

    async fn run_stage_and_commit(
        replies: Vec<(u16, &'static str)>,
        stage: impl Future<Output = anyhow::Result<i64>>,
        stopped: impl Future<Output = anyhow::Error>,
        still_active: anyhow::Result<()>,
        landed: bool,
    ) -> (anyhow::Result<(i64, Committed)>, Vec<String>) {
        let (base, server) = mock_play(replies).await;
        let http = reqwest::Client::new();
        let edits = PlayEdits::insert_at(&http, &base, &base, "token", "com.example")
            .await
            .unwrap();
        let result = stage_and_commit(
            &edits,
            stage,
            PublishGate {
                stopped,
                still_active: async { still_active },
            },
            async move { anyhow::Ok(landed) },
        )
        .await;
        (result, server.await.unwrap())
    }

    const OPEN_EDIT: (u16, &str) = (200, r#"{"id":"e1"}"#);
    const COMMIT: &str = "POST /com.example/edits/e1:commit ";
    const DELETE: &str = "DELETE /com.example/edits/e1 ";

    #[tokio::test]
    async fn a_build_stopped_mid_upload_discards_the_edit() {
        let (result, seen) = run_stage_and_commit(
            vec![OPEN_EDIT, (204, "")],
            std::future::pending(),
            async { anyhow::anyhow!("canceled") },
            Ok(()),
            false,
        )
        .await;
        let error = format!("{:#}", result.unwrap_err());
        assert!(error.contains("nothing was published"), "{error}");
        assert!(seen[1].starts_with(DELETE), "{seen:?}");
        assert!(!seen.iter().any(|line| line.starts_with(COMMIT)));
    }

    #[tokio::test]
    async fn a_build_that_stopped_before_the_commit_is_not_committed() {
        let (result, seen) = run_stage_and_commit(
            vec![OPEN_EDIT, (204, "")],
            async { anyhow::Ok(42) },
            std::future::pending(),
            Err(anyhow::anyhow!("canceled")),
            false,
        )
        .await;
        assert!(result.is_err());
        assert_eq!(seen.len(), 2, "{seen:?}");
        assert!(seen[1].starts_with(DELETE), "{seen:?}");
    }

    #[tokio::test]
    async fn a_rejected_commit_discards_the_edit() {
        let (result, seen) = run_stage_and_commit(
            vec![OPEN_EDIT, (400, r#"{"error":{"message":"no"}}"#), (204, "")],
            async { anyhow::Ok(42) },
            std::future::pending(),
            Ok(()),
            true,
        )
        .await;
        assert!(result.is_err());
        assert!(seen[1].starts_with(COMMIT), "{seen:?}");
        assert!(seen[2].starts_with(DELETE), "{seen:?}");
    }

    #[tokio::test]
    async fn a_lost_commit_reply_with_the_release_on_the_track_is_a_success() {
        let (result, seen) = run_stage_and_commit(
            vec![OPEN_EDIT, (0, "")],
            async { anyhow::Ok(42) },
            std::future::pending(),
            Ok(()),
            true,
        )
        .await;
        assert_eq!(result.unwrap(), (42, Committed::ReplyLost));
        assert_eq!(seen.len(), 2, "{seen:?}");
        assert!(seen[1].starts_with(COMMIT), "{seen:?}");
    }

    #[tokio::test]
    async fn missing_service_account_secret_fails_before_any_upload() {
        let root = workspace("secret");
        let error = publish_to_google_play(
            &reqwest::Client::new(),
            &root,
            &BTreeMap::new(),
            None,
            &config(),
            &mut Vec::new(),
            PublishGate {
                stopped: std::future::pending(),
                still_active: async { Ok(()) },
            },
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(error.contains("PLAY_SERVICE_ACCOUNT"), "{error}");
        fs::remove_dir_all(&root).ok();
    }
}
