//! `dart analyze --format=json` after the build's steps.
//!
//! The runner analyzes the app in its workspace once the steps have run,
//! whether they succeeded or a step failed, and reports a compact summary:
//! counts by severity and the most severe diagnostics. Analysis never
//! affects the build: a missing `dart`, unresolved packages or output that
//! does not parse only mark the `analyze` step skipped.

use std::path::PathBuf;

use oore_contract::{AnalyzerCounts, AnalyzerDiagnostic, AnalyzerSeverity, RunnerAnalyzerReport};
use serde::Deserialize;

use crate::test_reports::{strip_prefixes, truncate, workspace_prefixes};

/// The step's name in logs, checks and the build page.
pub(crate) const ANALYZE_STEP: &str = "analyze";
/// Pipeline env that turns analysis off: `false`, `0`, `no` or `off`.
pub(crate) const ANALYZE_ENV: &str = "OORE_ANALYZE";
/// Diagnostics sent per build; the counts still include every diagnostic.
pub(crate) const MAX_DIAGNOSTICS: usize = 50;
const MAX_MESSAGE_BYTES: usize = 1024;
const MAX_FILE_BYTES: usize = 512;
const MAX_CODE_BYTES: usize = 128;
/// Analyzer output read at most; a larger result is skipped.
pub(crate) const MAX_OUTPUT_BYTES: usize = 32 * 1024 * 1024;

/// Exit codes the script uses to say why it did not analyze.
const EXIT_NO_PUBSPEC: i32 = 65;
const EXIT_NO_PACKAGES: i32 = 66;
const EXIT_NO_DART: i32 = 127;

/// The command shown in the log for the step.
pub(crate) const ANALYZE_COMMAND: &str = "dart analyze --format=json";

/// Runs in the workspace root, the app directory. Analysis needs resolved
/// packages: without them every import is an error, which says nothing
/// about the change. Stderr goes to the log; stdout is the JSON.
pub(crate) fn analyze_script() -> String {
    format!(
        "[ -f pubspec.yaml ] || exit {EXIT_NO_PUBSPEC}\n\
         [ -f .dart_tool/package_config.json ] || exit {EXIT_NO_PACKAGES}\n\
         command -v dart >/dev/null 2>&1 || exit {EXIT_NO_DART}\n\
         exec {ANALYZE_COMMAND}\n"
    )
}

/// Whether the pipeline's env turned analysis on. Analysis is opt-in until it
/// has been tried on macOS runners and in VMs.
pub(crate) fn analysis_enabled(env: &[(String, String)]) -> bool {
    env.iter()
        .rev()
        .find(|(key, _)| key == ANALYZE_ENV)
        .is_some_and(|(_, value)| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "true" | "1" | "yes" | "on"
            )
        })
}

/// Why the step was skipped, for an exit code the script chose. `None` for
/// the analyzer's own exit codes, whose output is parsed.
pub(crate) fn skip_reason(exit_code: Option<i32>) -> Option<&'static str> {
    match exit_code {
        Some(EXIT_NO_PUBSPEC) => Some("no pubspec.yaml in the workspace root"),
        Some(EXIT_NO_PACKAGES) => {
            Some("packages are not resolved (no .dart_tool/package_config.json)")
        }
        Some(EXIT_NO_DART) => Some("dart is not on the PATH"),
        _ => None,
    }
}

#[derive(Deserialize)]
struct Output {
    diagnostics: Vec<RawDiagnostic>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDiagnostic {
    #[serde(default)]
    code: String,
    #[serde(default)]
    severity: String,
    #[serde(default)]
    problem_message: String,
    #[serde(default)]
    location: Option<RawLocation>,
}

#[derive(Deserialize)]
struct RawLocation {
    #[serde(default)]
    file: String,
    #[serde(default)]
    range: Option<RawRange>,
}

#[derive(Deserialize)]
struct RawRange {
    start: RawPosition,
}

#[derive(Deserialize)]
struct RawPosition {
    #[serde(default)]
    line: i64,
    #[serde(default)]
    column: i64,
}

fn severity(raw: &str) -> AnalyzerSeverity {
    match raw.to_ascii_uppercase().as_str() {
        "ERROR" => AnalyzerSeverity::Error,
        "WARNING" => AnalyzerSeverity::Warning,
        _ => AnalyzerSeverity::Info,
    }
}

/// Parse `dart analyze --format=json` output into a summary. The JSON is the
/// last line that holds a `diagnostics` object, since a login shell or the
/// SDK can print other lines first. Paths lose the workspace prefixes, and
/// `mask` hides secrets in messages.
pub(crate) fn parse_analyzer_output(
    stdout: &str,
    workspaces: &[PathBuf],
    mask: impl Fn(&str) -> String,
) -> Result<RunnerAnalyzerReport, String> {
    let output = stdout
        .lines()
        .rev()
        .map(str::trim)
        .filter(|line| line.starts_with('{') && line.contains("\"diagnostics\""))
        .find_map(|line| serde_json::from_str::<Output>(line).ok())
        // Output spread over several lines parses from its first brace.
        .or_else(|| {
            let start = stdout.find('{')?;
            serde_json::from_str::<Output>(stdout[start..].trim()).ok()
        })
        .ok_or_else(|| "the analyzer printed no JSON result".to_string())?;

    let prefixes: Vec<String> = workspaces
        .iter()
        .flat_map(|workspace| workspace_prefixes(workspace))
        .collect();
    let mut counts = AnalyzerCounts::default();
    let mut diagnostics: Vec<AnalyzerDiagnostic> = output
        .diagnostics
        .into_iter()
        .map(|raw| {
            let severity = severity(&raw.severity);
            match severity {
                AnalyzerSeverity::Error => counts.errors += 1,
                AnalyzerSeverity::Warning => counts.warnings += 1,
                AnalyzerSeverity::Info => counts.infos += 1,
            }
            let (file, line, column) = match raw.location {
                Some(location) => {
                    let (line, column) = location
                        .range
                        .map(|range| (range.start.line, range.start.column))
                        .unwrap_or_default();
                    (location.file, line, column)
                }
                None => (String::new(), 0, 0),
            };
            AnalyzerDiagnostic {
                severity,
                code: raw.code,
                message: raw.problem_message,
                file,
                line,
                column,
            }
        })
        .collect();
    diagnostics.sort_by(|a, b| {
        (a.severity, &a.file, a.line, a.column).cmp(&(b.severity, &b.file, b.line, b.column))
    });
    diagnostics.truncate(MAX_DIAGNOSTICS);
    for diagnostic in &mut diagnostics {
        diagnostic.file = truncate(
            strip_prefixes(&diagnostic.file, &prefixes).trim_start_matches('/'),
            MAX_FILE_BYTES,
        );
        diagnostic.code = truncate(&diagnostic.code, MAX_CODE_BYTES);
        diagnostic.message = truncate(&mask(&diagnostic.message), MAX_MESSAGE_BYTES);
    }
    Ok(RunnerAnalyzerReport {
        counts,
        diagnostics,
    })
}

fn plural(count: i64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

pub(crate) fn summary_line(report: &RunnerAnalyzerReport) -> String {
    let counts = report.counts;
    format!(
        "[oore-analyze] {}, {}, {}",
        plural(counts.errors, "error", "errors"),
        plural(counts.warnings, "warning", "warnings"),
        plural(counts.infos, "info", "infos"),
    )
}

/// Log lines for the most severe diagnostics, `file:line:column`.
pub(crate) fn diagnostic_lines(report: &RunnerAnalyzerReport, limit: usize) -> Vec<String> {
    report
        .diagnostics
        .iter()
        .take(limit)
        .map(|diagnostic| {
            let severity = match diagnostic.severity {
                AnalyzerSeverity::Error => "error",
                AnalyzerSeverity::Warning => "warning",
                AnalyzerSeverity::Info => "info",
            };
            format!(
                "[oore-analyze] {severity} {}:{}:{} {} ({})",
                diagnostic.file,
                diagnostic.line,
                diagnostic.column,
                diagnostic.message,
                diagnostic.code
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/dart_analyze.json");

    #[test]
    fn parses_counts_and_orders_diagnostics_by_severity() {
        let workspace = PathBuf::from("/Users/oore/oore-runner-workspaces/oore-build-abc");
        let report = parse_analyzer_output(FIXTURE, &[workspace], |text| text.to_string()).unwrap();
        assert_eq!(
            report.counts,
            AnalyzerCounts {
                errors: 1,
                warnings: 2,
                infos: 2
            }
        );
        let summary: Vec<(AnalyzerSeverity, &str, &str, i64)> = report
            .diagnostics
            .iter()
            .map(|d| (d.severity, d.code.as_str(), d.file.as_str(), d.line))
            .collect();
        assert_eq!(
            summary,
            [
                (
                    AnalyzerSeverity::Error,
                    "undefined_identifier",
                    "lib/src/login.dart",
                    42
                ),
                (
                    AnalyzerSeverity::Warning,
                    "unused_import",
                    "lib/main.dart",
                    3
                ),
                (
                    AnalyzerSeverity::Warning,
                    "dead_code",
                    "lib/src/login.dart",
                    10
                ),
                (
                    AnalyzerSeverity::Info,
                    "prefer_const_constructors",
                    "lib/main.dart",
                    12
                ),
                (
                    AnalyzerSeverity::Info,
                    "avoid_print",
                    "test/widget_test.dart",
                    7
                ),
            ]
        );
        assert_eq!(report.diagnostics[0].column, 5);
        assert_eq!(report.diagnostics[0].message, "Undefined name 'sessoin'.");
        assert_eq!(
            summary_line(&report),
            "[oore-analyze] 1 error, 2 warnings, 2 infos"
        );
        assert_eq!(
            diagnostic_lines(&report, 1),
            [
                "[oore-analyze] error lib/src/login.dart:42:5 Undefined name 'sessoin'. (undefined_identifier)"
            ]
        );
    }

    #[test]
    fn finds_the_json_after_other_output_and_masks_messages() {
        let stdout = format!(
            "Analyzing app...\n{}\n",
            r#"{"version":1,"diagnostics":[{"code":"todo","severity":"INFO","type":"TODO","location":{"file":"/w/lib/a.dart","range":{"start":{"offset":1,"line":2,"column":3},"end":{"offset":2,"line":2,"column":4}}},"problemMessage":"TODO: rotate s3cr3t"}]}"#
        );
        let report = parse_analyzer_output(&stdout, &[PathBuf::from("/w")], |text| {
            text.replace("s3cr3t", "***")
        })
        .unwrap();
        assert_eq!(report.counts.infos, 1);
        assert_eq!(report.diagnostics[0].file, "lib/a.dart");
        assert_eq!(report.diagnostics[0].message, "TODO: rotate ***");
    }

    #[test]
    fn pretty_printed_output_parses_too() {
        let pretty = serde_json::to_string_pretty(
            &serde_json::from_str::<serde_json::Value>(FIXTURE).unwrap(),
        )
        .unwrap();
        let report = parse_analyzer_output(&format!("Analyzing app...\n{pretty}\n"), &[], |t| {
            t.to_string()
        })
        .unwrap();
        assert_eq!(report.counts.errors, 1);
        assert_eq!(report.diagnostics.len(), 5);
    }

    #[test]
    fn a_clean_project_has_no_diagnostics() {
        let report =
            parse_analyzer_output(r#"{"version":1,"diagnostics":[]}"#, &[], |t| t.to_string())
                .unwrap();
        assert_eq!(report, RunnerAnalyzerReport::default());
    }

    #[test]
    fn output_without_json_is_an_error() {
        assert!(
            parse_analyzer_output("Analyzing app...\nNo issues found!\n", &[], |t| {
                t.to_string()
            })
            .is_err()
        );
        assert!(parse_analyzer_output("{\"diagnostics\": 3}", &[], |t| t.to_string()).is_err());
    }

    #[test]
    fn keeps_the_most_severe_diagnostics_and_counts_them_all() {
        let mut items = Vec::new();
        for index in 0..(MAX_DIAGNOSTICS + 10) {
            items.push(format!(
                r#"{{"code":"avoid_print","severity":"INFO","location":{{"file":"/w/lib/f{index:03}.dart","range":{{"start":{{"line":1,"column":1}}}}}},"problemMessage":"Don't invoke 'print'."}}"#
            ));
        }
        items.push(r#"{"code":"x","severity":"ERROR","problemMessage":"Broken."}"#.to_string());
        let stdout = format!(r#"{{"version":1,"diagnostics":[{}]}}"#, items.join(","));
        let report =
            parse_analyzer_output(&stdout, &[PathBuf::from("/w")], |t| t.to_string()).unwrap();
        assert_eq!(report.counts.infos, (MAX_DIAGNOSTICS + 10) as i64);
        assert_eq!(report.counts.errors, 1);
        assert_eq!(report.diagnostics.len(), MAX_DIAGNOSTICS);
        assert_eq!(report.diagnostics[0].severity, AnalyzerSeverity::Error);
        assert_eq!(report.diagnostics[0].file, "");
    }

    #[test]
    fn analysis_is_off_unless_the_pipeline_turns_it_on() {
        let env = |value: &str| vec![(ANALYZE_ENV.to_string(), value.to_string())];
        assert!(!analysis_enabled(&[]));
        assert!(!analysis_enabled(&env("false")));
        assert!(!analysis_enabled(&env("")));
        for on in ["true", "1", "Yes", " on "] {
            assert!(analysis_enabled(&env(on)), "{on}");
        }
    }

    #[test]
    fn script_exit_codes_name_the_skip_reason() {
        let script = analyze_script();
        assert!(script.contains("exec dart analyze --format=json"));
        assert!(
            skip_reason(Some(EXIT_NO_PACKAGES))
                .unwrap()
                .contains("not resolved")
        );
        assert!(skip_reason(Some(EXIT_NO_DART)).is_some());
        // The analyzer exits 1 to 3 when it finds issues.
        for code in [0, 1, 2, 3] {
            assert!(skip_reason(Some(code)).is_none());
        }
    }
}
