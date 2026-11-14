//! The one log line that explains a failed build, with the likely cause in
//! plain words, found by matching the failed step's log against the
//! failures Flutter builds commonly hit.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use oore_contract::{ApiError, BuildFailureHint, BuildFailureHintResponse, StepResult};
use sqlx::{Row, SqlitePool};
use tracing::error;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::logs::require_build_log_read;
use crate::util::api_err;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

/// Log rows read from the end of a build; the failure is near the end.
const MAX_LOG_ROWS: i64 = 20_000;
const MAX_LINE_CHARS: usize = 400;
const STEP_MARKER: &str = "[oore-step] ";

struct Pattern {
    /// Lowercase text the line contains.
    needle: &'static str,
    category: &'static str,
    cause: &'static str,
    fix: Option<&'static str>,
}

const PATTERNS: &[Pattern] = &[
    Pattern {
        needle: "no space left on device",
        category: "disk",
        cause: "The runner ran out of disk space.",
        fix: Some("Free space on the runner, or lower how long build files are kept."),
    },
    Pattern {
        needle: "errsecinternalcomponent",
        category: "signing",
        cause: "The keychain was locked when Xcode tried to sign.",
        fix: Some(
            "Run the build again. If it keeps happening, restart the runner so Oore can unlock its keychain.",
        ),
    },
    Pattern {
        needle: "no signing certificate",
        category: "signing",
        cause: "Xcode found no certificate to sign with.",
        fix: Some(
            "Check the pipeline's iOS signing: the certificate must match the profile's team and type.",
        ),
    },
    Pattern {
        needle: "provisioning profile",
        category: "signing",
        cause: "The provisioning profile does not fit this app or certificate.",
        fix: Some(
            "Check the bundle ID, team and expiry of the profile in the pipeline's iOS signing.",
        ),
    },
    Pattern {
        needle: "code signing is required",
        category: "signing",
        cause: "Xcode needs signing for this build and none was set up.",
        fix: Some("Add iOS signing to the pipeline, or build with --no-codesign."),
    },
    Pattern {
        needle: "keystore was tampered with",
        category: "signing",
        cause: "The Android keystore password is wrong.",
        fix: Some("Re-enter the keystore and key passwords in the pipeline's Android signing."),
    },
    Pattern {
        needle: "cocoapods could not find compatible versions",
        category: "cocoapods",
        cause: "CocoaPods could not find versions of the pods that fit together.",
        fix: Some(
            "Run `pod repo update` or delete ios/Podfile.lock, then raise the iOS deployment target if a pod needs it.",
        ),
    },
    Pattern {
        needle: "error running pod install",
        category: "cocoapods",
        cause: "pod install failed.",
        fix: Some("Run `pod install --repo-update` in ios/ locally to see the full error."),
    },
    Pattern {
        needle: "version solving failed",
        category: "dart",
        cause: "pub could not find package versions that fit together.",
        fix: Some(
            "Loosen the version constraint named above, or run `flutter pub upgrade` locally.",
        ),
    },
    Pattern {
        needle: "the current dart sdk version is",
        category: "flutter",
        cause: "A package needs a newer Flutter than the runner has.",
        fix: Some("Pin a newer Flutter for the pipeline, or update Flutter on the runner."),
    },
    Pattern {
        needle: "unsupported class file major version",
        category: "gradle",
        cause: "Gradle and the runner's Java version do not fit together.",
        fix: Some("Use Java 17 on the runner, or upgrade the Gradle wrapper."),
    },
    Pattern {
        needle: "java heap space",
        category: "gradle",
        cause: "Gradle ran out of memory.",
        fix: Some("Raise org.gradle.jvmargs in android/gradle.properties, for example -Xmx4g."),
    },
    Pattern {
        needle: "sdk location not found",
        category: "gradle",
        cause: "Gradle could not find the Android SDK.",
        fix: Some("Set ANDROID_HOME on the runner."),
    },
    Pattern {
        needle: "license for package",
        category: "gradle",
        cause: "An Android SDK license has not been accepted on the runner.",
        fix: Some("Run `flutter doctor --android-licenses` on the runner."),
    },
    Pattern {
        needle: "could not resolve all",
        category: "gradle",
        cause: "Gradle could not download a dependency.",
        fix: Some(
            "Check the version and repository of the dependency named above, and the runner's network.",
        ),
    },
    Pattern {
        needle: "requires a newer version of xcode",
        category: "xcode",
        cause: "The project needs a newer Xcode than the runner has.",
        fix: Some("Update Xcode on the runner, or pick a runner with a newer one."),
    },
    Pattern {
        needle: "unable to find a destination",
        category: "xcode",
        cause: "Xcode has no simulator or device for the requested destination.",
        fix: Some("Install the iOS platform in Xcode on the runner."),
    },
    Pattern {
        needle: "swift compiler error",
        category: "xcode",
        cause: "Swift code failed to compile.",
        fix: None,
    },
    Pattern {
        needle: "framework not found",
        category: "xcode",
        cause: "The linker could not find a framework.",
        fix: Some("Run pod install again, or check the framework is added to the target."),
    },
    Pattern {
        needle: "could not resolve host",
        category: "network",
        cause: "The runner could not reach a server.",
        fix: Some("Check the runner's network and DNS, then run the build again."),
    },
    Pattern {
        needle: "connection timed out",
        category: "network",
        cause: "A download timed out.",
        fix: Some("Run the build again; check the runner's network if it keeps happening."),
    },
];

fn trimmed(line: &str) -> String {
    let line = line.trim();
    if line.chars().count() <= MAX_LINE_CHARS {
        return line.to_string();
    }
    line.chars().take(MAX_LINE_CHARS).collect::<String>() + "…"
}

fn hint(
    step: Option<&str>,
    line: &str,
    sequence: i64,
    category: &str,
    cause: &str,
    fix: Option<&str>,
) -> BuildFailureHint {
    BuildFailureHint {
        step: step.map(str::to_string),
        line: trimmed(line),
        sequence,
        category: category.to_string(),
        cause: cause.to_string(),
        fix: fix.map(str::to_string),
    }
}

fn step_started(line: &str) -> Option<String> {
    let json = line.trim().strip_prefix(STEP_MARKER)?;
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    (value.get("event")?.as_str()? == "start")
        .then(|| value.get("name")?.as_str().map(str::to_string))?
}

/// A Dart compile error: `lib/main.dart:12:5: Error: ...`.
fn is_dart_error(lower: &str) -> bool {
    lower
        .split_once(".dart:")
        .is_some_and(|(_, rest)| rest.contains(": error:"))
}

/// Finds the line that explains the failure. `lines` are (sequence, text)
/// in log order; only lines of `failed_step` count when it is known.
pub(crate) fn find_hint<'a>(
    lines: impl IntoIterator<Item = (i64, &'a str)>,
    failed_step: Option<&str>,
) -> Option<BuildFailureHint> {
    let mut current: Option<String> = None;
    let mut known: Option<BuildFailureHint> = None;
    let mut generic: Option<BuildFailureHint> = None;
    let mut gradle_wrong_next = false;
    for (sequence, text) in lines {
        for line in text.lines() {
            if let Some(name) = step_started(line) {
                current = Some(name);
                gradle_wrong_next = false;
                continue;
            }
            if failed_step.is_some() && current.as_deref() != failed_step {
                continue;
            }
            let step = current.as_deref();
            let lower = line.trim().to_lowercase();
            if lower.is_empty() {
                continue;
            }
            if gradle_wrong_next {
                gradle_wrong_next = false;
                if known.is_none() {
                    if let Some(pattern) = PATTERNS.iter().find(|p| lower.contains(p.needle)) {
                        known = Some(hint(
                            step,
                            line,
                            sequence,
                            pattern.category,
                            pattern.cause,
                            pattern.fix,
                        ));
                    } else {
                        known = Some(hint(
                            step,
                            line,
                            sequence,
                            "gradle",
                            "A Gradle task failed.",
                            None,
                        ));
                    }
                }
                continue;
            }
            if lower == "* what went wrong:" {
                gradle_wrong_next = true;
                continue;
            }
            if known.is_none() {
                if let Some(pattern) = PATTERNS.iter().find(|p| lower.contains(p.needle)) {
                    known = Some(hint(
                        step,
                        line,
                        sequence,
                        pattern.category,
                        pattern.cause,
                        pattern.fix,
                    ));
                } else if is_dart_error(&lower) {
                    known = Some(hint(
                        step,
                        line,
                        sequence,
                        "dart",
                        "Dart code failed to compile.",
                        None,
                    ));
                }
            }
            if generic.is_none()
                && (lower.starts_with("error")
                    || lower.contains(" error:")
                    || lower.starts_with("failure:"))
            {
                generic = Some(hint(
                    step,
                    line,
                    sequence,
                    "other",
                    "The step stopped at this error.",
                    None,
                ));
            }
        }
    }
    known.or(generic)
}

/// `GET /v1/builds/{build_id}/failure-hint`
pub async fn get_build_failure_hint(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
) -> ApiResult<BuildFailureHintResponse> {
    require_build_log_read(&state.db, &auth.0, &build_id).await?;
    load_failure_hint(&state.db, &build_id)
        .await
        .map(|hint| Json(BuildFailureHintResponse { hint }))
        .map_err(|e| {
            error!(error = %e, "Failed to read the build log");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to read the build log",
            )
        })
}

pub(crate) async fn load_failure_hint(
    pool: &SqlitePool,
    build_id: &str,
) -> sqlx::Result<Option<BuildFailureHint>> {
    let row = sqlx::query("SELECT status, step_results FROM builds WHERE id = ?1")
        .bind(build_id)
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else { return Ok(None) };
    let status: String = row.get("status");
    if !matches!(status.as_str(), "failed" | "timed_out") {
        return Ok(None);
    }
    let steps: Vec<StepResult> = row
        .get::<Option<String>, _>("step_results")
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    let failed_step = steps
        .iter()
        .rev()
        .find(|step| step.status == "failed")
        .map(|step| step.name.clone());
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT sequence, content FROM (SELECT sequence, content FROM build_logs \
         WHERE build_id = ?1 ORDER BY sequence DESC LIMIT ?2) ORDER BY sequence",
    )
    .bind(build_id)
    .bind(MAX_LOG_ROWS)
    .fetch_all(pool)
    .await?;
    let lines = || {
        rows.iter()
            .map(|(sequence, content)| (*sequence, content.as_str()))
    };
    // The step's start marker may be older than the rows read; then any
    // line counts.
    let marker_seen = failed_step.as_deref().is_some_and(|name| {
        lines().any(|(_, text)| {
            text.lines()
                .any(|line| step_started(line).as_deref() == Some(name))
        })
    });
    Ok(find_hint(
        lines(),
        failed_step.as_deref().filter(|_| marker_seen),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start(name: &str) -> String {
        format!(
            "{STEP_MARKER}{}",
            serde_json::json!({ "event": "start", "name": name, "command": "x" })
        )
    }

    #[test]
    fn gradle_what_went_wrong_names_the_next_line() {
        let log = [
            start("Build Android"),
            "FAILURE: Build failed with an exception.".to_string(),
            "* What went wrong:".to_string(),
            "Execution failed for task ':app:mergeReleaseResources'.".to_string(),
        ];
        let hint = find_hint(
            log.iter().enumerate().map(|(i, l)| (i as i64, l.as_str())),
            Some("Build Android"),
        )
        .unwrap();
        assert_eq!(hint.category, "gradle");
        assert_eq!(hint.sequence, 3);
        assert_eq!(hint.step.as_deref(), Some("Build Android"));
    }

    #[test]
    fn a_known_failure_beats_an_earlier_generic_error() {
        let log = [
            start("Install pods"),
            "error: something generic".to_string(),
            "[!] CocoaPods could not find compatible versions for pod \"Firebase\"".to_string(),
        ];
        let hint = find_hint(
            log.iter().enumerate().map(|(i, l)| (i as i64, l.as_str())),
            Some("Install pods"),
        )
        .unwrap();
        assert_eq!(hint.category, "cocoapods");
        assert!(hint.fix.is_some());
    }

    #[test]
    fn only_the_failed_step_counts() {
        let log = [
            start("Analyze"),
            "error: warning treated as error".to_string(),
            start("Build iOS"),
            "lib/main.dart:12:5: Error: Undefined name 'foo'.".to_string(),
        ];
        let hint = find_hint(
            log.iter().enumerate().map(|(i, l)| (i as i64, l.as_str())),
            Some("Build iOS"),
        )
        .unwrap();
        assert_eq!(hint.category, "dart");
        assert_eq!(hint.sequence, 3);
    }

    #[test]
    fn no_line_stands_out_gives_no_hint() {
        let log = [
            start("Build"),
            "Compiling...".to_string(),
            "Done".to_string(),
        ];
        assert!(
            find_hint(
                log.iter().enumerate().map(|(i, l)| (i as i64, l.as_str())),
                Some("Build")
            )
            .is_none()
        );
    }
}
