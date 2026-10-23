//! Test results from build steps.
//!
//! A `flutter test` or `dart test` step gets `--file-reporter json:<file>` so
//! the log keeps the readable output while the machine-readable event stream
//! lands in the per-build reports directory. Steps can also write JUnit XML
//! files to `$OORE_TEST_REPORTS_DIR`. After each step the runner parses and
//! removes every report in that directory.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use oore_contract::{RunnerTestFailure, RunnerTestReport};

pub(crate) const TEST_REPORTS_DIR_ENV: &str = "OORE_TEST_REPORTS_DIR";
/// Reports directory, relative to the build workspace.
pub(crate) const TEST_REPORTS_SUBDIR: &str = ".oore/test-reports";

/// Failures sent per report; totals still count every failure.
const MAX_FAILURES_PER_REPORT: usize = 200;
const MAX_MESSAGE_BYTES: usize = 2048;
const MAX_STACK_TRACE_BYTES: usize = 4096;
pub(crate) const MAX_REPORT_FILE_BYTES: u64 = 64 * 1024 * 1024;

const TEST_COMMANDS: [&[&str]; 4] = [
    &["flutter", "test"],
    &["fvm", "flutter", "test"],
    &["dart", "test"],
    &["fvm", "dart", "test"],
];

/// Add a JSON file reporter to a plain `flutter test` or `dart test` command.
/// Compound shell commands and commands that already pick a file reporter
/// are left alone; they can write reports to `$OORE_TEST_REPORTS_DIR`.
pub(crate) fn with_test_file_reporter(command: &str, report_path: &Path) -> String {
    let is_compound = ["&&", "||", ";", "|", ">", "<", "`", "$(", "\n", "&"]
        .iter()
        .any(|token| command.contains(token));
    let args: Vec<&str> = command.split_whitespace().collect();
    let is_test = TEST_COMMANDS.iter().any(|prefix| args.starts_with(prefix));
    let has_file_reporter = args.iter().any(|arg| arg.starts_with("--file-reporter"));
    if is_compound || !is_test || has_file_reporter {
        return command.to_string();
    }
    let reporter = format!("--file-reporter=json:{}", report_path.display());
    format!("{} {}", command.trim_end(), shell_quote(&reporter))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Parse and remove every report in `dir`. Returns `None` when the step
/// wrote no reports. Paths under any of `workspaces` (the host workspace and,
/// for a VM build, its guest path) are made relative. Failure text goes
/// through `mask` before it is cut to size, so a secret that straddles the
/// cut is still masked whole.
pub(crate) fn collect_test_reports(
    dir: &Path,
    workspaces: &[PathBuf],
    step: &str,
    mask: impl Fn(&str) -> String,
) -> Option<RunnerTestReport> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_file())
        .collect();
    files.sort();

    let prefixes: Vec<String> = workspaces
        .iter()
        .flat_map(|workspace| workspace_prefixes(workspace))
        .collect();
    let mut report = RunnerTestReport {
        step: step.to_string(),
        ..Default::default()
    };
    let mut found = false;
    for file in files {
        let parsed = std::fs::metadata(&file)
            .ok()
            .filter(|meta| meta.len() <= MAX_REPORT_FILE_BYTES)
            .and_then(|_| std::fs::read_to_string(&file).ok())
            .and_then(|raw| match file.extension().and_then(|ext| ext.to_str()) {
                Some("json") => parse_dart_json_report(&raw),
                Some("xml") => parse_junit_report(&raw),
                _ => None,
            });
        let _ = std::fs::remove_file(&file);
        let Some(parsed) = parsed else { continue };
        found = true;
        report.total += parsed.total;
        report.passed += parsed.passed;
        report.failed += parsed.failed;
        report.skipped += parsed.skipped;
        report.duration_ms += parsed.duration_ms;
        report.failures.extend(parsed.failures);
    }
    if !found {
        return None;
    }
    report.failures.truncate(MAX_FAILURES_PER_REPORT);
    for failure in &mut report.failures {
        failure.suite = strip_prefixes(&failure.suite, &prefixes);
        failure.name = strip_prefixes(&failure.name, &prefixes);
        failure.message = failure
            .message
            .take()
            .map(|message| truncate(&mask(&message), MAX_MESSAGE_BYTES));
        failure.stack_trace = failure
            .stack_trace
            .take()
            .map(|stack| truncate(&mask(&stack), MAX_STACK_TRACE_BYTES));
    }
    Some(report)
}

pub(crate) fn summary_line(report: &RunnerTestReport) -> String {
    format!(
        "[oore-tests] {} tests: {} passed, {} failed, {} skipped",
        report.total, report.passed, report.failed, report.skipped
    )
}

/// macOS temp paths often appear both as `/var/...` and `/private/var/...`.
fn workspace_prefixes(workspace: &Path) -> Vec<String> {
    let mut prefixes = vec![format!("{}/", workspace.display())];
    if let Ok(canonical) = workspace.canonicalize() {
        let canonical = format!("{}/", canonical.display());
        if !prefixes.contains(&canonical) {
            prefixes.push(canonical);
        }
    }
    prefixes
}

fn strip_prefixes(value: &str, prefixes: &[String]) -> String {
    let mut value = value.to_string();
    for prefix in prefixes {
        value = value.replace(prefix.as_str(), "");
    }
    value
}

fn truncate(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

fn join_nonempty(parts: &[String]) -> Option<String> {
    let parts: Vec<&str> = parts
        .iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join("\n\n"))
}

/// Parse the `dart test` JSON reporter protocol, which `flutter test
/// --machine` and `--file-reporter json:` both emit: one event per line.
/// Returns `None` for JSON that is not a test event stream.
pub(crate) fn parse_dart_json_report(raw: &str) -> Option<RunnerTestReport> {
    struct TestStart {
        name: String,
        suite_id: i64,
        started_ms: i64,
    }
    let mut suites: HashMap<i64, String> = HashMap::new();
    let mut tests: HashMap<i64, TestStart> = HashMap::new();
    let mut errors: HashMap<i64, (Vec<String>, Vec<String>)> = HashMap::new();
    let mut report = RunnerTestReport::default();
    let mut last_time = 0;
    let mut is_test_stream = false;

    for line in raw.lines() {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
            continue;
        };
        if let Some(time) = event.get("time").and_then(|t| t.as_i64()) {
            last_time = last_time.max(time);
        }
        let time = event.get("time").and_then(|t| t.as_i64()).unwrap_or(0);
        match event.get("type").and_then(|t| t.as_str()) {
            Some("start") => is_test_stream = true,
            Some("suite") => {
                let suite = &event["suite"];
                if let (Some(id), Some(path)) = (suite["id"].as_i64(), suite["path"].as_str()) {
                    suites.insert(id, path.to_string());
                }
            }
            Some("testStart") => {
                let test = &event["test"];
                if let Some(id) = test["id"].as_i64() {
                    tests.insert(
                        id,
                        TestStart {
                            name: test["name"].as_str().unwrap_or_default().to_string(),
                            suite_id: test["suiteID"].as_i64().unwrap_or(-1),
                            started_ms: time,
                        },
                    );
                }
            }
            Some("error") => {
                if let Some(id) = event["testID"].as_i64() {
                    let entry = errors.entry(id).or_default();
                    entry
                        .0
                        .push(event["error"].as_str().unwrap_or_default().to_string());
                    entry
                        .1
                        .push(event["stackTrace"].as_str().unwrap_or_default().to_string());
                }
            }
            Some("testDone") => {
                let Some(id) = event["testID"].as_i64() else {
                    continue;
                };
                let result = event["result"].as_str().unwrap_or("success");
                // Hidden tests are synthetic, such as suite loading; they only
                // matter when they fail.
                if event["hidden"].as_bool().unwrap_or(false) && result == "success" {
                    continue;
                }
                report.total += 1;
                if event["skipped"].as_bool().unwrap_or(false) {
                    report.skipped += 1;
                } else if result == "success" {
                    report.passed += 1;
                } else {
                    report.failed += 1;
                    let test = tests.get(&id);
                    let (messages, stacks) = errors.remove(&id).unwrap_or_default();
                    report.failures.push(RunnerTestFailure {
                        suite: test
                            .and_then(|test| suites.get(&test.suite_id))
                            .cloned()
                            .unwrap_or_default(),
                        name: test.map(|test| test.name.clone()).unwrap_or_default(),
                        message: join_nonempty(&messages),
                        stack_trace: join_nonempty(&stacks),
                        duration_ms: test.map(|test| (time - test.started_ms).max(0)),
                    });
                }
            }
            _ => {}
        }
    }
    report.duration_ms = last_time;
    is_test_stream.then_some(report)
}

/// Parse a JUnit XML report. Returns `None` when the file is not XML.
pub(crate) fn parse_junit_report(raw: &str) -> Option<RunnerTestReport> {
    use xmlparser::{ElementEnd, Token, Tokenizer};

    #[derive(Default)]
    struct Case {
        suite: String,
        name: String,
        duration_ms: Option<i64>,
        failed: bool,
        skipped: bool,
        messages: Vec<String>,
        details: Vec<String>,
    }

    let mut report = RunnerTestReport::default();
    let mut suites: Vec<String> = Vec::new();
    let mut element = String::new();
    let mut attrs: HashMap<String, String> = HashMap::new();
    let mut case: Option<Case> = None;
    let mut capturing = false;
    let mut saw_testcase_root = false;

    let finish = |case: Case, report: &mut RunnerTestReport| {
        report.total += 1;
        report.duration_ms += case.duration_ms.unwrap_or(0);
        if case.failed {
            report.failed += 1;
            report.failures.push(RunnerTestFailure {
                suite: case.suite,
                name: case.name,
                message: join_nonempty(&case.messages),
                stack_trace: join_nonempty(&case.details),
                duration_ms: case.duration_ms,
            });
        } else if case.skipped {
            report.skipped += 1;
        } else {
            report.passed += 1;
        }
    };

    for token in Tokenizer::from(raw) {
        match token.ok()? {
            Token::ElementStart { local, .. } => {
                element = local.as_str().to_string();
                attrs.clear();
            }
            Token::Attribute { local, value, .. } => {
                attrs.insert(local.as_str().to_string(), unescape(value.as_str()));
            }
            Token::ElementEnd { end, .. } => match end {
                ElementEnd::Open | ElementEnd::Empty => {
                    let empty = matches!(end, ElementEnd::Empty);
                    match element.as_str() {
                        "testsuites" => saw_testcase_root = true,
                        "testsuite" => {
                            saw_testcase_root = true;
                            if !empty {
                                suites.push(attrs.get("name").cloned().unwrap_or_default());
                            }
                        }
                        "testcase" => {
                            saw_testcase_root = true;
                            let suite = attrs
                                .get("classname")
                                .filter(|name| !name.is_empty())
                                .cloned()
                                .or_else(|| suites.last().cloned())
                                .unwrap_or_default();
                            let new_case = Case {
                                suite,
                                name: attrs.get("name").cloned().unwrap_or_default(),
                                duration_ms: attrs
                                    .get("time")
                                    .and_then(|time| time.trim().parse::<f64>().ok())
                                    .map(|seconds| (seconds * 1000.0).round() as i64),
                                ..Default::default()
                            };
                            if empty {
                                finish(new_case, &mut report);
                            } else {
                                case = Some(new_case);
                            }
                        }
                        "failure" | "error" => {
                            if let Some(case) = case.as_mut() {
                                case.failed = true;
                                if let Some(message) = attrs.get("message") {
                                    case.messages.push(message.clone());
                                }
                                capturing = !empty;
                            }
                        }
                        "skipped" => {
                            if let Some(case) = case.as_mut() {
                                case.skipped = true;
                            }
                        }
                        _ => {}
                    }
                }
                ElementEnd::Close(_, local) => match local.as_str() {
                    "testsuite" => {
                        suites.pop();
                    }
                    "testcase" => {
                        if let Some(case) = case.take() {
                            finish(case, &mut report);
                        }
                    }
                    "failure" | "error" => capturing = false,
                    _ => {}
                },
            },
            Token::Text { text } if capturing => {
                if let Some(case) = case.as_mut() {
                    case.details.push(unescape(text.as_str()));
                }
            }
            Token::Cdata { text, .. } if capturing => {
                if let Some(case) = case.as_mut() {
                    case.details.push(text.as_str().to_string());
                }
            }
            _ => {}
        }
    }
    saw_testcase_root.then_some(report)
}

fn unescape(value: &str) -> String {
    if !value.contains('&') {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest.find(';') else { break };
        let entity = &rest[1..end];
        let decoded = match entity {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(ch) => {
                out.push(ch);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const DART_JSON: &str = r#"{"protocolVersion":"0.1.1","runnerVersion":null,"pid":1,"type":"start","time":0}
{"suite":{"id":0,"platform":"vm","path":"/ws/test/login_test.dart"},"type":"suite","time":0}
{"test":{"id":1,"name":"loading /ws/test/login_test.dart","suiteID":0,"groupIDs":[],"metadata":{"skip":false,"skipReason":null}},"type":"testStart","time":1}
{"testID":1,"result":"success","skipped":false,"hidden":true,"type":"testDone","time":200}
{"test":{"id":3,"name":"login shows an error for a bad password","suiteID":0,"groupIDs":[2],"metadata":{"skip":false}},"type":"testStart","time":210}
{"testID":3,"error":"Expected: 'Wrong password'\n  Actual: 'Welcome'","stackTrace":"package:app/login_test.dart 12:5","isFailure":true,"type":"error","time":250}
{"testID":3,"result":"failure","skipped":false,"hidden":false,"type":"testDone","time":260}
{"test":{"id":4,"name":"login signs in","suiteID":0,"groupIDs":[2],"metadata":{"skip":false}},"type":"testStart","time":261}
{"testID":4,"result":"success","skipped":false,"hidden":false,"type":"testDone","time":300}
{"test":{"id":5,"name":"login remembers me","suiteID":0,"groupIDs":[2],"metadata":{"skip":true}},"type":"testStart","time":301}
{"testID":5,"result":"success","skipped":true,"hidden":false,"type":"testDone","time":302}
{"suite":{"id":6,"platform":"vm","path":"/ws/test/broken_test.dart"},"type":"suite","time":303}
{"test":{"id":7,"name":"loading /ws/test/broken_test.dart","suiteID":6,"groupIDs":[],"metadata":{"skip":false}},"type":"testStart","time":303}
{"testID":7,"error":"Failed to load: compile error","stackTrace":"","isFailure":false,"type":"error","time":400}
{"testID":7,"result":"error","skipped":false,"hidden":false,"type":"testDone","time":401}
{"success":false,"type":"done","time":450}
"#;

    #[test]
    fn dart_json_counts_tests_and_keeps_failure_messages() {
        let report = parse_dart_json_report(DART_JSON).unwrap();
        assert_eq!(
            (report.total, report.passed, report.failed, report.skipped),
            (4, 1, 2, 1)
        );
        assert_eq!(report.duration_ms, 450);
        let failure = &report.failures[0];
        assert_eq!(failure.suite, "/ws/test/login_test.dart");
        assert_eq!(failure.name, "login shows an error for a bad password");
        assert_eq!(
            failure.message.as_deref(),
            Some("Expected: 'Wrong password'\n  Actual: 'Welcome'")
        );
        assert_eq!(
            failure.stack_trace.as_deref(),
            Some("package:app/login_test.dart 12:5")
        );
        assert_eq!(failure.duration_ms, Some(50));
        assert_eq!(report.failures[1].name, "loading /ws/test/broken_test.dart");
        assert_eq!(report.failures[1].stack_trace, None);
    }

    #[test]
    fn junit_reads_failures_errors_and_skips() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites>
  <testsuite name="LoginTests" tests="4">
    <testcase classname="app.LoginTests" name="signs in" time="0.25"/>
    <testcase name="rejects &quot;bad&quot; password" time="1.5">
      <failure message="expected &lt;error&gt;" type="AssertionError"><![CDATA[at LoginTests.kt:12]]></failure>
    </testcase>
    <testcase name="crashes"><error message="boom">trace &amp; more</error></testcase>
    <testcase name="later"><skipped/></testcase>
  </testsuite>
</testsuites>"#;
        let report = parse_junit_report(xml).unwrap();
        assert_eq!(
            (report.total, report.passed, report.failed, report.skipped),
            (4, 1, 2, 1)
        );
        assert_eq!(report.duration_ms, 1750);
        let failure = &report.failures[0];
        assert_eq!(failure.suite, "LoginTests");
        assert_eq!(failure.name, "rejects \"bad\" password");
        assert_eq!(failure.message.as_deref(), Some("expected <error>"));
        assert_eq!(failure.stack_trace.as_deref(), Some("at LoginTests.kt:12"));
        assert_eq!(
            report.failures[1].stack_trace.as_deref(),
            Some("trace & more")
        );
    }

    #[test]
    fn non_junit_xml_is_ignored() {
        assert!(parse_junit_report("<plist><dict/></plist>").is_none());
        assert!(parse_junit_report("not xml <<<").is_none());
        assert!(parse_dart_json_report(r#"{"coverage":[]}"#).is_none());
    }

    #[test]
    fn file_reporter_is_added_only_to_plain_test_commands() {
        let path = Path::new("/tmp/reports/build-2.json");
        assert_eq!(
            with_test_file_reporter("fvm flutter test --coverage", path),
            "fvm flutter test --coverage '--file-reporter=json:/tmp/reports/build-2.json'"
        );
        assert_eq!(
            with_test_file_reporter("dart test test/unit", path),
            "dart test test/unit '--file-reporter=json:/tmp/reports/build-2.json'"
        );
        for unchanged in [
            "fvm flutter build apk",
            "fvm flutter test && echo done",
            "fvm flutter test | tee out.txt",
            "fvm flutter test --file-reporter json:mine.json",
            "melos run test",
        ] {
            assert_eq!(with_test_file_reporter(unchanged, path), unchanged);
        }
    }

    #[test]
    fn collect_merges_reports_strips_workspace_and_clears_dir() {
        let root = std::env::temp_dir().join(format!("oore-test-reports-{}", std::process::id()));
        let dir = root.join("reports");
        let workspace = root.join("ws");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(&workspace).unwrap();
        let ws = workspace.display().to_string();
        std::fs::write(dir.join("a.json"), DART_JSON.replace("/ws", &ws)).unwrap();
        std::fs::write(
            dir.join("b.xml"),
            r#"<testsuite name="s"><testcase name="x"/></testsuite>"#,
        )
        .unwrap();
        std::fs::write(dir.join("notes.txt"), "ignored").unwrap();

        let workspaces = [workspace];
        let report = collect_test_reports(&dir, &workspaces, "build-2", str::to_string).unwrap();
        assert_eq!(report.step, "build-2");
        assert_eq!((report.total, report.passed, report.failed), (5, 2, 2));
        assert_eq!(report.failures[0].suite, "test/login_test.dart");
        assert_eq!(report.failures[1].name, "loading test/broken_test.dart");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        assert!(collect_test_reports(&dir, &workspaces, "build-3", str::to_string).is_none());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn secrets_are_masked_before_failure_text_is_cut() {
        let dir = std::env::temp_dir().join(format!("oore-test-mask-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let secret = "sk_live_0123456789abcdef";
        // The secret starts 8 bytes before the cut.
        let message = format!("{}{secret} tail", "x".repeat(MAX_MESSAGE_BYTES - 8));
        std::fs::write(
            dir.join("r.xml"),
            format!(
                r#"<testsuite name="s"><testcase name="t"><failure message="{message}">{message}</failure></testcase></testsuite>"#
            ),
        )
        .unwrap();
        let report =
            collect_test_reports(&dir, &[], "build-1", |text| text.replace(secret, "***")).unwrap();
        let failure = &report.failures[0];
        for text in [&failure.message, &failure.stack_trace] {
            let text = text.as_deref().unwrap();
            assert!(!text.contains(&secret[..8]), "secret prefix leaked");
            assert!(text.contains("***"));
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unescape_handles_numeric_and_unknown_entities() {
        assert_eq!(unescape("a &#65;&#x42; &bogus; &"), "a AB &bogus; &");
    }
}
