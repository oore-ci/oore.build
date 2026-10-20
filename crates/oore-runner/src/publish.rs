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
    play_track_name, read_aab_manifest,
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

/// Upload the build's signed App Bundle to Google Play and roll it out.
/// Progress lines go to `log`; they pass through the build's secret masker.
pub(crate) async fn publish_to_google_play(
    http: &reqwest::Client,
    workspace: &Path,
    secrets: &BTreeMap<String, String>,
    changelog: Option<&str>,
    config: &GooglePlayPublishConfig,
    log: &mut Vec<String>,
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
    let uploaded = async {
        let version_code = edits.upload_bundle(&bundle).await?;
        log.push(format!(
            "[oore-publish] Uploaded {} as versionCode {version_code}",
            bundle.strip_prefix(workspace).unwrap_or(&bundle).display()
        ));
        match &mapping {
            Some(path) => {
                edits.upload_mapping(version_code, path).await?;
                log.push("[oore-publish] Uploaded mapping.txt".to_string());
            }
            None if config.upload_mapping => {
                log.push(
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
    }
    .await;
    let version_code = match uploaded {
        Ok(version_code) => version_code,
        Err(error) => {
            // Leave nothing half-staged in Play Console.
            let _ = edits.delete().await;
            return Err(error);
        }
    };
    edits.commit().await?;

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
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(error.contains("PLAY_SERVICE_ACCOUNT"), "{error}");
        fs::remove_dir_all(&root).ok();
    }
}
