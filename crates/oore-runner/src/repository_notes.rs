//! Release notes the repository keeps: the section of a changelog file
//! headed with the app version, or a directory of `<locale>.txt` files.
//! The runner reads them at the build's commit and reports them with the
//! build's identity, so the notes always match the commit that was built.

use std::fs;
use std::path::Path;

use oore_contract::pipeline_config::ReleaseNotesSpec;
use oore_contract::{ReleaseNotesLocale, RepositoryReleaseNotes};

use crate::app_store_upload::read_release_notes_dir;

/// The longest notes any store takes (App Store Connect).
const MAX_NOTES_CHARS: usize = 4000;
const DEFAULT_LOCALE: &str = "en-US";

/// Reads the notes for `version` from where `spec` points. `Ok(None)` when
/// the file has no section for the version or the directory holds no notes.
pub(crate) fn read(
    workspace: &Path,
    spec: &ReleaseNotesSpec,
    version: &str,
) -> anyhow::Result<Option<RepositoryReleaseNotes>> {
    if let Some(path) = &spec.changelog {
        let file = workspace.join(path);
        let canonical_workspace = workspace.canonicalize()?;
        let Ok(canonical) = file.canonicalize() else {
            anyhow::bail!("release_notes.changelog {path} is not in the repository");
        };
        if !canonical.starts_with(&canonical_workspace) {
            anyhow::bail!("release_notes.changelog {path} is outside the repository");
        }
        let raw = fs::read_to_string(&canonical)?;
        return Ok(
            changelog_section(&raw, version).map(|text| RepositoryReleaseNotes {
                path: path.clone(),
                text: limit(&text),
                locales: Vec::new(),
            }),
        );
    }
    let Some(dir) = &spec.dir else {
        return Ok(None);
    };
    let Some(mut notes) = read_release_notes_dir(workspace, dir)? else {
        return Ok(None);
    };
    let default = notes
        .iter()
        .position(|note| note.locale == DEFAULT_LOCALE)
        .unwrap_or(0);
    let first = notes.remove(default);
    Ok(Some(RepositoryReleaseNotes {
        path: dir.clone(),
        text: limit(&first.whats_new),
        locales: notes
            .into_iter()
            .map(|note| ReleaseNotesLocale {
                locale: note.locale,
                text: limit(&note.whats_new),
            })
            .collect(),
    }))
}

fn limit(text: &str) -> String {
    text.trim().chars().take(MAX_NOTES_CHARS).collect()
}

/// The body of the Markdown section headed with `version`, such as
/// `## 2.5.1`, `## [2.5.1] - 2026-11-20` or `# v2.5.1`, up to the next
/// heading of the same or a higher level. `None` when there is no such
/// section or it is empty.
pub(crate) fn changelog_section(raw: &str, version: &str) -> Option<String> {
    let version = version.trim();
    if version.is_empty() {
        return None;
    }
    let mut lines = raw.lines();
    let level = loop {
        let line = lines.next()?;
        if let Some((level, title)) = heading(line)
            && heading_names_version(title, version)
        {
            break level;
        }
    };
    let mut body = Vec::new();
    for line in lines {
        if heading(line).is_some_and(|(next, _)| next <= level) {
            break;
        }
        body.push(line);
    }
    let text = body.join("\n").trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn heading(line: &str) -> Option<(usize, &str)> {
    let level = line.chars().take_while(|c| *c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let title = &line[level..];
    title.starts_with(' ').then(|| (level, title.trim()))
}

fn heading_names_version(title: &str, version: &str) -> bool {
    let title = title.trim_start_matches('[');
    let title = title
        .strip_prefix('v')
        .or_else(|| title.strip_prefix('V'))
        .unwrap_or(title);
    let Some(rest) = title.strip_prefix(version) else {
        return false;
    };
    // `2.5.1` must not match `2.5.10` or `2.5.1-beta`.
    rest.chars()
        .next()
        .is_none_or(|next| matches!(next, ']' | ' ' | '(' | '+' | ':'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHANGELOG: &str = "# Changelog\n\n## [Unreleased]\n\n- Work in progress\n\n## [2.5.10] - 2026-12-01\n\n- Later\n\n## [2.5.1] - 2026-11-20\n\n### Fixed\n\n- Login on slow networks\n- Crash on Android 9\n\n## 2.5.0\n\n- First\n";

    #[test]
    fn reads_the_section_for_the_version() {
        assert_eq!(
            changelog_section(CHANGELOG, "2.5.1").as_deref(),
            Some("### Fixed\n\n- Login on slow networks\n- Crash on Android 9")
        );
        assert_eq!(
            changelog_section(CHANGELOG, "2.5.0").as_deref(),
            Some("- First")
        );
        assert_eq!(
            changelog_section(CHANGELOG, "2.5.10").as_deref(),
            Some("- Later")
        );
    }

    #[test]
    fn accepts_a_v_prefix_and_rejects_longer_versions() {
        assert_eq!(
            changelog_section("# v3.0.0\nBig\n# v2.0.0\nOld", "3.0.0").as_deref(),
            Some("Big")
        );
        assert_eq!(changelog_section("## 2.5.1-beta\n- x\n", "2.5.1"), None);
        assert_eq!(changelog_section(CHANGELOG, "9.9.9"), None);
        assert_eq!(
            changelog_section("## 2.5.1\n\n## 2.5.0\n- x", "2.5.1"),
            None
        );
    }

    #[test]
    fn reads_a_changelog_file_and_a_locale_directory() {
        let workspace =
            std::env::temp_dir().join(format!("oore-repository-notes-{}", std::process::id()));
        let _ = fs::remove_dir_all(&workspace);
        fs::create_dir_all(workspace.join("notes")).unwrap();
        fs::write(workspace.join("CHANGELOG.md"), CHANGELOG).unwrap();
        fs::write(workspace.join("notes/en-US.txt"), "Faster sync\n").unwrap();
        fs::write(workspace.join("notes/de-DE.txt"), "Schneller\n").unwrap();

        let from_file = read(
            &workspace,
            &ReleaseNotesSpec {
                changelog: Some("CHANGELOG.md".into()),
                dir: None,
            },
            "2.5.0",
        )
        .unwrap()
        .unwrap();
        assert_eq!(from_file.path, "CHANGELOG.md");
        assert_eq!(from_file.text, "- First");

        let from_dir = read(
            &workspace,
            &ReleaseNotesSpec {
                changelog: None,
                dir: Some("notes".into()),
            },
            "2.5.0",
        )
        .unwrap()
        .unwrap();
        assert_eq!(from_dir.text, "Faster sync");
        assert_eq!(
            from_dir.locales,
            vec![ReleaseNotesLocale {
                locale: "de-DE".into(),
                text: "Schneller".into()
            }]
        );
        let _ = fs::remove_dir_all(&workspace);
    }
}
