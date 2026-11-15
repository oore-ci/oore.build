//! App Store pre-flight for the IPA's version and build number: what App
//! Store Connect refuses at upload time, found before the upload, with the
//! fix for how the project numbers its builds.

use std::cmp::Ordering;
use std::fs;
use std::path::Path;

use oore_contract::RunnerAppStorePreflightResponse;

/// One pre-flight finding: a stable code, a short title, and the sentence
/// with the fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NumberFinding {
    pub code: &'static str,
    pub title: String,
    pub detail: String,
}

/// Where the IPA's build number (CFBundleVersion) came from, which decides
/// the fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BuildNumberSource {
    /// The `+N` of pubspec.yaml's `version:`, as Flutter builds by default.
    Pubspec,
    /// The build command's `--build-number`, such as `$OORE_VERSION_CODE`.
    BuildCommand,
    /// A native project: Xcode's build setting.
    Xcode,
}

fn pubspec_version(workspace: &Path) -> Option<String> {
    let raw = fs::read_to_string(workspace.join("pubspec.yaml")).ok()?;
    let value = raw.lines().find_map(|line| line.strip_prefix("version:"))?;
    let value = value.split(" #").next()?.trim().trim_matches(['"', '\'']);
    (!value.is_empty()).then(|| value.to_string())
}

/// Where `build_number` came from: pubspec.yaml when it matches the `+N`
/// there (or pubspec has none and Flutter used the version name), the build
/// command when a Flutter project's number differs, Xcode otherwise.
pub(crate) fn build_number_source(workspace: &Path, build_number: &str) -> BuildNumberSource {
    if !workspace.join("pubspec.yaml").is_file() {
        return BuildNumberSource::Xcode;
    }
    let Some(version) = pubspec_version(workspace) else {
        return BuildNumberSource::Pubspec;
    };
    let pubspec_number = match version.split_once('+') {
        Some((_, number)) => number.trim(),
        None => version.as_str(),
    };
    if pubspec_number == build_number.trim() {
        BuildNumberSource::Pubspec
    } else {
        BuildNumberSource::BuildCommand
    }
}

/// Whether App Store Connect takes `value` as a CFBundleVersion: one to
/// three whole numbers separated by dots.
pub(crate) fn is_valid_bundle_version(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    (1..=3).contains(&parts.len())
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn numbers(value: &str) -> Vec<u64> {
    value
        .split('.')
        .map(|part| part.trim().parse().unwrap_or(0))
        .collect()
}

/// Compares dotted version or build numbers the way Apple does: part by
/// part as whole numbers, a missing part counting as 0.
pub(crate) fn compare_dotted(a: &str, b: &str) -> Ordering {
    let (a, b) = (numbers(a), numbers(b));
    let len = a.len().max(b.len());
    (0..len)
        .map(|i| {
            a.get(i)
                .copied()
                .unwrap_or(0)
                .cmp(&b.get(i).copied().unwrap_or(0))
        })
        .find(|ordering| ordering.is_ne())
        .unwrap_or(Ordering::Equal)
}

/// The next build number after `highest`: its last part plus one.
fn next_number(highest: &str) -> String {
    let mut parts = numbers(highest);
    if let Some(last) = parts.last_mut() {
        *last += 1;
    }
    parts
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(".")
}

/// The next patch version after `version`, such as 2.5.2 after 2.5.1.
fn next_version(version: &str) -> String {
    let mut parts = numbers(version);
    parts.resize(parts.len().max(3), 0);
    parts[2] += 1;
    parts
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(".")
}

/// How to give the build a number above `highest`.
pub(crate) fn raise_build_number_advice(
    source: BuildNumberSource,
    version: &str,
    build_number: &str,
    highest: &str,
) -> String {
    let next = next_number(highest);
    match source {
        BuildNumberSource::Pubspec => format!(
            "Bump the +N in pubspec.yaml to {next} or higher (version: {version}+{next}) and commit it."
        ),
        BuildNumberSource::BuildCommand => {
            let raise = match (highest.parse::<i64>(), build_number.parse::<i64>()) {
                (Ok(highest), Ok(current)) => {
                    format!(
                        " If it passes $OORE_VERSION_CODE, raise build_number_offset by at least {}.",
                        highest - current + 1
                    )
                }
                _ => String::new(),
            };
            format!(
                "The build command sets this number with --build-number, not pubspec.yaml. Make it {next} or higher.{raise}"
            )
        }
        BuildNumberSource::Xcode => format!(
            "Raise the build number (CFBundleVersion, Current Project Version in Xcode) to {next} or higher."
        ),
    }
}

/// How to give the app a version above `above`.
fn raise_version_advice(source: BuildNumberSource, above: &str) -> String {
    let next = next_version(above);
    match source {
        BuildNumberSource::Xcode => format!(
            "Raise the version (CFBundleShortVersionString, Marketing Version in Xcode) to {next} or higher."
        ),
        _ => format!(
            "Raise the version in pubspec.yaml to {next} or higher (version: {next}+1) and commit it."
        ),
    }
}

/// Checks the IPA's own build number.
pub(crate) fn local_findings(
    source: BuildNumberSource,
    version: &str,
    build_number: &str,
) -> Vec<NumberFinding> {
    if is_valid_bundle_version(build_number) {
        return Vec::new();
    }
    let fix = match source {
        BuildNumberSource::Xcode => {
            "Set Current Project Version in Xcode to a whole number such as 12.".to_string()
        }
        _ => format!("Set version: {version}+N in pubspec.yaml, with N a whole number such as 12."),
    };
    vec![NumberFinding {
        code: "asc_build_number_format",
        title: "The build number is not one App Store Connect accepts".to_string(),
        detail: format!(
            "The build number (CFBundleVersion) is \"{build_number}\", which App Store Connect does not accept: it must be one to three whole numbers separated by dots, such as 12 or 1.0.12. {fix}"
        ),
    }]
}

/// Version states in which Apple takes no new builds for a version: it is
/// approved, releasing or released.
fn version_closed(state: &str) -> Option<&'static str> {
    Some(match state {
        "READY_FOR_DISTRIBUTION" | "READY_FOR_SALE" | "PREORDER_READY_FOR_SALE" => {
            "live on the App Store"
        }
        "PENDING_DEVELOPER_RELEASE" | "PENDING_APPLE_RELEASE" | "ACCEPTED" => {
            "approved and waiting to be released"
        }
        "PROCESSING_FOR_DISTRIBUTION" | "PROCESSING_FOR_APP_STORE" => "being released",
        "REPLACED_WITH_NEW_VERSION" | "REMOVED_FROM_SALE" | "DEVELOPER_REMOVED_FROM_SALE" => {
            "released"
        }
        _ => return None,
    })
}

/// Checks the IPA's version and build number against what App Store Connect
/// has.
pub(crate) fn store_findings(
    source: BuildNumberSource,
    version: &str,
    build_number: &str,
    store: &RunnerAppStorePreflightResponse,
) -> Vec<NumberFinding> {
    let mut findings = Vec::new();
    if store.unavailable.is_some() || !store.app_found {
        return findings;
    }
    let closed = store.version_state.as_deref().and_then(version_closed);
    if let Some(closed) = closed {
        findings.push(NumberFinding {
            code: "asc_version_released",
            title: format!("Version {version} is already {closed}"),
            detail: format!(
                "Version {version} is {closed}, so App Store Connect takes no new builds for it. {}",
                raise_version_advice(source, version)
            ),
        });
    } else if let Some(live) = store.live_version.as_deref()
        && compare_dotted(version, live) == Ordering::Less
    {
        findings.push(NumberFinding {
            code: "asc_version_lower",
            title: format!("Version {version} is lower than the live {live}"),
            detail: format!(
                "Version {version} is lower than {live}, the version live on the App Store, and App Store Connect only takes higher versions. {}",
                raise_version_advice(source, live)
            ),
        });
    }
    if closed.is_none()
        && let Some(highest) = store.highest_build_number.as_deref()
        && compare_dotted(build_number, highest) != Ordering::Greater
    {
        findings.push(NumberFinding {
            code: "asc_build_number_used",
            title: format!("Build number {build_number} is already used for {version}"),
            detail: format!(
                "Build number {build_number} is already used: App Store Connect has {version} ({highest}), and each upload of {version} needs a higher number. {}",
                raise_build_number_advice(source, version, build_number, highest)
            ),
        });
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(name: &str, pubspec: Option<&str>) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("oore-asc-preflight-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        if let Some(pubspec) = pubspec {
            fs::write(dir.join("pubspec.yaml"), pubspec).unwrap();
        }
        dir
    }

    fn store(
        highest: Option<&str>,
        state: Option<&str>,
        live: Option<&str>,
    ) -> RunnerAppStorePreflightResponse {
        RunnerAppStorePreflightResponse {
            app_found: true,
            highest_build_number: highest.map(str::to_string),
            version_state: state.map(str::to_string),
            live_version: live.map(str::to_string),
            unavailable: None,
        }
    }

    #[test]
    fn tells_where_the_build_number_came_from() {
        let dir = workspace("pubspec", Some("name: app\nversion: 2.5.1+12 # store\n"));
        assert_eq!(build_number_source(&dir, "12"), BuildNumberSource::Pubspec);
        assert_eq!(
            build_number_source(&dir, "147"),
            BuildNumberSource::BuildCommand
        );
        let dir = workspace("no-number", Some("name: app\nversion: 2.5.1\n"));
        assert_eq!(
            build_number_source(&dir, "2.5.1"),
            BuildNumberSource::Pubspec
        );
        let dir = workspace("native", None);
        assert_eq!(build_number_source(&dir, "12"), BuildNumberSource::Xcode);
    }

    #[test]
    fn accepts_only_apples_build_number_format() {
        for valid in ["12", "1.0.12", "2.5"] {
            assert!(is_valid_bundle_version(valid), "{valid}");
        }
        for invalid in ["", "1.0.0.1", "12a", "1..2", "$(FLUTTER_BUILD_NUMBER)"] {
            assert!(!is_valid_bundle_version(invalid), "{invalid}");
        }
        let findings = local_findings(BuildNumberSource::Pubspec, "2.5.1", "");
        assert!(
            findings[0]
                .detail
                .contains("version: 2.5.1+N in pubspec.yaml")
        );
    }

    #[test]
    fn compares_like_apple() {
        assert_eq!(compare_dotted("1.0.10", "1.0.9"), Ordering::Greater);
        assert_eq!(compare_dotted("2.5", "2.5.0"), Ordering::Equal);
        assert_eq!(compare_dotted("12", "12"), Ordering::Equal);
        assert_eq!(compare_dotted("2.4.9", "2.5.0"), Ordering::Less);
    }

    #[test]
    fn a_used_build_number_says_to_bump_pubspec() {
        let findings = store_findings(
            BuildNumberSource::Pubspec,
            "2.5.1",
            "12",
            &store(Some("12"), Some("PREPARE_FOR_SUBMISSION"), Some("2.5.0")),
        );
        assert_eq!(findings.len(), 1);
        assert!(
            findings[0]
                .detail
                .contains("Build number 12 is already used")
        );
        assert!(
            findings[0]
                .detail
                .contains("Bump the +N in pubspec.yaml to 13 or higher (version: 2.5.1+13)")
        );
        assert!(!findings[0].detail.contains("build_number_offset"));
    }

    #[test]
    fn a_build_command_number_says_to_raise_the_offset() {
        let findings = store_findings(
            BuildNumberSource::BuildCommand,
            "2.5.1",
            "140",
            &store(Some("147"), None, None),
        );
        assert!(
            findings[0]
                .detail
                .contains("raise build_number_offset by at least 8")
        );
    }

    #[test]
    fn a_higher_build_number_passes() {
        assert!(
            store_findings(
                BuildNumberSource::Pubspec,
                "2.5.1",
                "13",
                &store(Some("12"), Some("PREPARE_FOR_SUBMISSION"), Some("2.5.0")),
            )
            .is_empty()
        );
        assert!(
            store_findings(
                BuildNumberSource::Pubspec,
                "2.5.1",
                "1",
                &store(None, None, None)
            )
            .is_empty()
        );
    }

    #[test]
    fn a_live_version_takes_no_new_builds() {
        let findings = store_findings(
            BuildNumberSource::Pubspec,
            "2.5.1",
            "13",
            &store(Some("12"), Some("READY_FOR_DISTRIBUTION"), Some("2.5.1")),
        );
        assert_eq!(findings.len(), 1);
        assert!(
            findings[0]
                .detail
                .contains("Version 2.5.1 is live on the App Store")
        );
        assert!(findings[0].detail.contains("version: 2.5.2+1"));
    }

    #[test]
    fn a_version_below_the_live_one_is_refused() {
        let findings = store_findings(
            BuildNumberSource::Xcode,
            "2.4.0",
            "5",
            &store(None, None, Some("2.5.1")),
        );
        assert_eq!(findings.len(), 1);
        assert!(findings[0].detail.contains("lower than 2.5.1"));
        assert!(findings[0].detail.contains("Marketing Version in Xcode"));
    }

    #[test]
    fn nothing_is_found_when_the_store_was_not_read() {
        let mut unread = store(Some("99"), Some("READY_FOR_SALE"), None);
        unread.unavailable = Some("timeout".to_string());
        assert!(store_findings(BuildNumberSource::Pubspec, "1.0.0", "1", &unread).is_empty());
    }
}
