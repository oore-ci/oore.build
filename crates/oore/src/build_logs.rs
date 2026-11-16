//! Build logs from the terminal: `oore build logs`, `oore build why`, and
//! the live log stream that `oore build watch` reads step changes from.

use anyhow::Context;
use clap::Args;
use oore_contract::{
    BuildDetailResponse, BuildFailureHint, BuildFailureHintResponse, BuildLogChunk,
    BuildLogsResponse, BuildTestResultsResponse,
};

use crate::build_commands::{Api, ApiConnectionArgs};
use crate::cli_contract::{CliError, exit, print_json, print_json_line};

const STEP_MARKER: &str = "[oore-step] ";
const LOG_PAGE: usize = 5000;
const FINISHED: [&str; 5] = ["succeeded", "failed", "canceled", "timed_out", "expired"];

/// One event of a server-sent event stream.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct SseEvent {
    pub(crate) event: String,
    pub(crate) data: String,
    pub(crate) id: Option<String>,
}

/// Splits a byte stream into server-sent events.
#[derive(Debug, Default)]
pub(crate) struct SseParser {
    buffer: String,
}

impl SseParser {
    pub(crate) fn push(&mut self, bytes: &[u8]) -> Vec<SseEvent> {
        self.buffer
            .push_str(&String::from_utf8_lossy(bytes).replace("\r\n", "\n"));
        let mut events = Vec::new();
        while let Some(end) = self.buffer.find("\n\n") {
            let block: String = self.buffer.drain(..end + 2).collect();
            let mut event = SseEvent {
                event: "message".to_string(),
                ..SseEvent::default()
            };
            let mut data = Vec::new();
            for line in block.lines() {
                if line.starts_with(':') {
                    continue;
                }
                let (field, value) = line.split_once(':').unwrap_or((line, ""));
                let value = value.strip_prefix(' ').unwrap_or(value);
                match field {
                    "event" => event.event = value.to_string(),
                    "data" => data.push(value),
                    "id" => event.id = Some(value.to_string()),
                    _ => {}
                }
            }
            if data.is_empty() && event.event == "message" {
                continue;
            }
            event.data = data.join("\n");
            events.push(event);
        }
        events
    }
}

/// The live log stream of one build, `GET /v1/builds/{id}/logs/stream`.
pub(crate) struct LogStream {
    response: reqwest::Response,
    parser: SseParser,
    pending: std::collections::VecDeque<SseEvent>,
}

impl LogStream {
    /// Opens the stream, resuming after `after` when given.
    pub(crate) async fn open(
        api: &Api,
        build_id: &str,
        after: Option<i64>,
    ) -> anyhow::Result<Self> {
        let path = api.url_with(&["v1", "builds", build_id, "logs", "stream"])?;
        let mut request = api
            .client
            .get(path)
            .header(reqwest::header::ACCEPT, "text/event-stream");
        if let Some(after) = after {
            request = request.header("Last-Event-ID", after.to_string());
        }
        let response = api.execute(request).await?;
        if !response.status().is_success() {
            return Err(crate::build_commands::api_failure(response, "log stream")
                .await
                .into());
        }
        Ok(Self {
            response,
            parser: SseParser::default(),
            pending: std::collections::VecDeque::new(),
        })
    }

    /// The next event, or `None` when the stream ends.
    pub(crate) async fn next(&mut self) -> anyhow::Result<Option<SseEvent>> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                return Ok(Some(event));
            }
            let Some(bytes) = self
                .response
                .chunk()
                .await
                .context("the log stream was cut")?
            else {
                return Ok(None);
            };
            self.pending.extend(self.parser.push(&bytes));
        }
    }
}

/// A `[oore-step]` line the runner writes when a step starts or ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StepMarker {
    pub(crate) name: String,
    /// `None` when the step started, else how it ended.
    pub(crate) status: Option<String>,
    pub(crate) exit_code: Option<i64>,
}

impl StepMarker {
    pub(crate) fn parse(line: &str) -> Option<Self> {
        let json = line.trim().strip_prefix(STEP_MARKER)?;
        let value: serde_json::Value = serde_json::from_str(json).ok()?;
        let name = value.get("name")?.as_str()?.to_string();
        match value.get("event")?.as_str()? {
            "start" => Some(Self {
                name,
                status: None,
                exit_code: None,
            }),
            "end" => Some(Self {
                name,
                status: Some(
                    value
                        .get("status")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("finished")
                        .to_string(),
                ),
                exit_code: value.get("exit_code").and_then(serde_json::Value::as_i64),
            }),
            _ => None,
        }
    }

    pub(crate) fn state(&self) -> &str {
        self.status.as_deref().unwrap_or("started")
    }

    pub(crate) fn describe(&self) -> String {
        match (&self.status, self.exit_code) {
            (None, _) => format!("{} started", self.name),
            (Some(status), Some(code)) if code != 0 => {
                format!("{} {status} (exit {code})", self.name)
            }
            (Some(status), _) => format!("{} {status}", self.name),
        }
    }
}

/// One line of a build's log, with the step it belongs to.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct LogLine {
    pub(crate) sequence: i64,
    pub(crate) stream: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) step: Option<String>,
    pub(crate) text: String,
}

/// Splits chunks into lines and names each line's step from the markers.
/// `current` carries the step across calls.
pub(crate) fn log_lines(chunks: &[BuildLogChunk], current: &mut Option<String>) -> Vec<LogLine> {
    let mut lines = Vec::new();
    for chunk in chunks {
        for text in chunk.content.lines() {
            if let Some(marker) = StepMarker::parse(text) {
                if marker.status.is_none() {
                    *current = Some(marker.name.clone());
                }
                lines.push(LogLine {
                    sequence: chunk.sequence,
                    stream: chunk.stream.clone(),
                    step: Some(marker.name.clone()),
                    text: format!("==> {}", marker.describe()),
                });
                continue;
            }
            lines.push(LogLine {
                sequence: chunk.sequence,
                stream: chunk.stream.clone(),
                step: current.clone(),
                text: text.to_string(),
            });
        }
    }
    lines
}

/// The filters `oore build logs` applies.
#[derive(Debug, Default, Clone)]
pub(crate) struct LogFilter {
    pub(crate) step: Option<String>,
    pub(crate) search: Option<String>,
    pub(crate) tail: Option<usize>,
}

impl LogFilter {
    fn keeps(&self, line: &LogLine) -> bool {
        let step_ok = self.step.as_deref().is_none_or(|wanted| {
            line.step
                .as_deref()
                .is_some_and(|step| step.eq_ignore_ascii_case(wanted.trim()))
        });
        let search_ok = self.search.as_deref().is_none_or(|wanted| {
            line.text
                .to_lowercase()
                .contains(&wanted.trim().to_lowercase())
        });
        step_ok && search_ok
    }

    /// The kept lines, and whether `tail` cut some.
    pub(crate) fn apply(&self, lines: Vec<LogLine>) -> (Vec<LogLine>, bool) {
        let mut kept: Vec<LogLine> = lines.into_iter().filter(|line| self.keeps(line)).collect();
        let mut truncated = false;
        if let Some(tail) = self.tail
            && kept.len() > tail
        {
            kept.drain(..kept.len() - tail);
            truncated = true;
        }
        (kept, truncated)
    }
}

/// Every stored log chunk of a build, page by page. With `server_filter`,
/// the daemon applies the step, tail and search filters.
async fn fetch_chunks(
    api: &Api,
    build_id: &str,
    server_filter: Option<&LogFilter>,
) -> anyhow::Result<Vec<BuildLogChunk>> {
    let path = api.url_with(&["v1", "builds", build_id, "logs"])?;
    let mut chunks: Vec<BuildLogChunk> = Vec::new();
    loop {
        let mut request = api
            .client
            .get(&path)
            .query(&[("limit", LOG_PAGE.to_string())]);
        if let Some(last) = chunks.last() {
            request = request.query(&[("after_sequence", last.sequence.to_string())]);
        }
        if let Some(filter) = server_filter {
            if let Some(step) = &filter.step {
                request = request.query(&[("step", step)]);
            }
            if let Some(search) = &filter.search {
                request = request.query(&[("search", search)]);
            }
            if let Some(tail) = filter.tail {
                request = request.query(&[("tail", tail.to_string())]);
            }
        }
        let page: BuildLogsResponse = api.send(request, "log lookup").await?;
        let full = page.logs.len() >= LOG_PAGE;
        chunks.extend(page.logs);
        // A filtered answer is one page.
        if !full || server_filter.is_some_and(|filter| filter.tail.is_some()) {
            return Ok(chunks);
        }
    }
}

#[derive(Debug, Args)]
pub(crate) struct BuildLogsArgs {
    /// Build whose log to print.
    build_id: String,

    /// Keep printing new lines until the build finishes, then exit as
    /// `oore build watch` does.
    #[arg(long, short = 'f')]
    follow: bool,

    /// Only the lines of this step, such as "Build Android".
    #[arg(long)]
    step: Option<String>,

    /// Only the last N lines.
    #[arg(long, value_name = "N")]
    tail: Option<usize>,

    /// Only lines that contain this text, ignoring case.
    #[arg(long)]
    search: Option<String>,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print `{"build_id", "lines", "truncated"}`. With -f, one JSON line
    /// per log line instead.
    #[arg(long)]
    json: bool,
}

fn print_lines(lines: &[LogLine], json_lines: bool) -> anyhow::Result<()> {
    for line in lines {
        if json_lines {
            print_json_line(line)?;
        } else {
            println!("{}", line.text);
        }
    }
    Ok(())
}

pub(crate) async fn handle_build_logs(args: BuildLogsArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let build_id = args.build_id.trim().to_string();
    let filter = LogFilter {
        step: args.step.clone().filter(|step| !step.trim().is_empty()),
        search: args
            .search
            .clone()
            .filter(|search| !search.trim().is_empty()),
        tail: args.tail,
    };
    let filtered = filter.step.is_some() || filter.search.is_some() || filter.tail.is_some();
    let server_filters = filtered
        && api
            .features()
            .await
            .iter()
            .any(|feature| feature == "log_filters");
    let chunks = fetch_chunks(&api, &build_id, server_filters.then_some(&filter)).await?;
    let mut current = None;
    let lines = log_lines(&chunks, &mut current);
    let (lines, truncated) = if server_filters {
        (lines, false)
    } else {
        filter.apply(lines)
    };
    if args.json && !args.follow {
        return print_json(&serde_json::json!({
            "build_id": build_id,
            "lines": lines,
            "truncated": truncated,
        }));
    }
    print_lines(&lines, args.json)?;
    if !args.follow {
        return Ok(());
    }
    let follow_filter = LogFilter {
        tail: None,
        ..filter
    };
    let mut last = chunks.last().map(|chunk| chunk.sequence);
    let mut stream = LogStream::open(&api, &build_id, last).await?;
    while let Some(event) = stream.next().await? {
        if event.event == "done" {
            break;
        }
        if event.event != "log" {
            continue;
        }
        let Ok(chunk) = serde_json::from_str::<BuildLogChunk>(&event.data) else {
            continue;
        };
        if last.is_some_and(|last| chunk.sequence <= last) {
            continue;
        }
        last = Some(chunk.sequence);
        let (lines, _) = follow_filter.apply(log_lines(&[chunk], &mut current));
        print_lines(&lines, args.json)?;
    }
    let detail: BuildDetailResponse = api
        .get(&format!("/v1/builds/{build_id}"), "build lookup")
        .await?;
    let build = detail.build;
    if build.status == "succeeded" {
        return Ok(());
    }
    Err(CliError::outcome(
        exit::FAILED,
        "build_not_succeeded",
        format!(
            "Build #{} {}",
            build.build_number,
            build.status.replace('_', " ")
        ),
    )
    .into())
}

#[derive(Debug, Args)]
pub(crate) struct BuildWhyArgs {
    /// Build to explain.
    build_id: String,

    /// Lines of the failed step to show.
    #[arg(long, default_value = "40", value_parser = clap::value_parser!(u64).range(1..=200))]
    tail: u64,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the diagnosis as JSON, the shape of `GET /v1/builds/{id}/diagnosis`.
    #[arg(long)]
    json: bool,
}

/// Why a build failed, as `GET /v1/builds/{id}/diagnosis` answers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct BuildDiagnosis {
    pub(crate) build_id: String,
    pub(crate) build_number: i64,
    pub(crate) status: String,
    #[serde(default)]
    pub(crate) failed_step: Option<FailedStep>,
    #[serde(default)]
    pub(crate) hint: Option<BuildFailureHint>,
    #[serde(default)]
    pub(crate) log_tail: Vec<String>,
    #[serde(default)]
    pub(crate) log_tail_truncated: bool,
    #[serde(default)]
    pub(crate) failed_tests: Vec<FailedTest>,
    #[serde(default)]
    pub(crate) failed_tests_total: i64,
    #[serde(default)]
    pub(crate) url: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct FailedStep {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) label: Option<String>,
    #[serde(default)]
    pub(crate) exit_code: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct FailedTest {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) message: Option<String>,
}

/// Builds the diagnosis from the separate calls, for a daemon without
/// `GET /v1/builds/{id}/diagnosis`.
async fn diagnose_from_parts(
    api: &Api,
    build_id: &str,
    tail: usize,
) -> anyhow::Result<BuildDiagnosis> {
    let detail: BuildDetailResponse = api
        .get(&format!("/v1/builds/{build_id}"), "build lookup")
        .await?;
    let build = detail.build;
    let mut diagnosis = BuildDiagnosis {
        build_id: build.id.clone(),
        build_number: build.build_number,
        status: build.status.clone(),
        failed_step: None,
        hint: None,
        log_tail: Vec::new(),
        log_tail_truncated: false,
        failed_tests: Vec::new(),
        failed_tests_total: 0,
        url: None,
    };
    if !matches!(build.status.as_str(), "failed" | "timed_out") {
        return Ok(diagnosis);
    }
    diagnosis.failed_step = build.step_results.as_ref().and_then(|steps| {
        steps
            .iter()
            .find(|step| step.status == "failed")
            .map(|step| FailedStep {
                name: step.name.clone(),
                label: None,
                exit_code: step.exit_code.map(i64::from),
            })
    });
    let hint: Option<BuildFailureHintResponse> = api
        .send_if_supported(
            api.client
                .get(api.url(&format!("/v1/builds/{build_id}/failure-hint"))),
            "failure hint",
        )
        .await
        .unwrap_or(None);
    diagnosis.hint = hint.and_then(|response| response.hint);
    let tests: Option<BuildTestResultsResponse> = api
        .send_if_supported(
            api.client
                .get(api.url(&format!("/v1/builds/{build_id}/tests"))),
            "test results",
        )
        .await
        .unwrap_or(None);
    if let Some(tests) = tests {
        diagnosis.failed_tests_total = i64::try_from(tests.failures.len()).unwrap_or(0);
        diagnosis.failed_tests = tests
            .failures
            .into_iter()
            .take(20)
            .map(|failure| FailedTest {
                name: if failure.suite.is_empty() {
                    failure.name
                } else {
                    format!("{} {}", failure.suite, failure.name)
                },
                message: failure.message,
            })
            .collect();
    }
    let chunks = fetch_chunks(api, build_id, None).await?;
    let mut current = None;
    let step = diagnosis
        .failed_step
        .as_ref()
        .map(|step| step.name.clone())
        .or_else(|| diagnosis.hint.as_ref().and_then(|hint| hint.step.clone()));
    let lines = log_lines(&chunks, &mut current);
    let has_step = step
        .as_deref()
        .is_some_and(|step| lines.iter().any(|line| line.step.as_deref() == Some(step)));
    let filter = LogFilter {
        step: step.filter(|_| has_step),
        search: None,
        tail: Some(tail),
    };
    let (lines, truncated) = filter.apply(lines);
    diagnosis.log_tail = lines.into_iter().map(|line| line.text).collect();
    diagnosis.log_tail_truncated = truncated;
    Ok(diagnosis)
}

pub(crate) fn render_diagnosis(diagnosis: &BuildDiagnosis) -> String {
    let status = diagnosis.status.replace('_', " ");
    let mut out = format!("Build #{} {status}", diagnosis.build_number);
    if !matches!(diagnosis.status.as_str(), "failed" | "timed_out") {
        out.push_str(if FINISHED.contains(&diagnosis.status.as_str()) {
            "; it did not fail, so there is nothing to explain.\n"
        } else {
            "; it has not finished.\n"
        });
        return out;
    }
    match &diagnosis.failed_step {
        Some(step) => {
            out.push_str(&format!(
                " in step {}",
                step.label.as_deref().unwrap_or(&step.name)
            ));
            if let Some(code) = step.exit_code {
                out.push_str(&format!(" (exit {code})"));
            }
            out.push('\n');
        }
        None => out.push('\n'),
    }
    if let Some(hint) = &diagnosis.hint {
        out.push_str(&format!("\nCause: {}\n", hint.cause));
        if let Some(fix) = &hint.fix {
            out.push_str(&format!("Fix: {fix}\n"));
        }
        out.push_str(&format!("Line: {}\n", hint.line));
    }
    if !diagnosis.failed_tests.is_empty() {
        out.push_str(&format!(
            "\n{} failed {}:\n",
            diagnosis.failed_tests_total,
            if diagnosis.failed_tests_total == 1 {
                "test"
            } else {
                "tests"
            }
        ));
        for test in &diagnosis.failed_tests {
            match &test.message {
                Some(message) => {
                    let first = message.lines().next().unwrap_or_default();
                    out.push_str(&format!("  {}: {first}\n", test.name));
                }
                None => out.push_str(&format!("  {}\n", test.name)),
            }
        }
    }
    if !diagnosis.log_tail.is_empty() {
        out.push_str(&format!(
            "\nLast {} lines{}:\n",
            diagnosis.log_tail.len(),
            if diagnosis.log_tail_truncated {
                " of the step"
            } else {
                ""
            }
        ));
        for line in &diagnosis.log_tail {
            out.push_str(&format!("  {line}\n"));
        }
    }
    if let Some(url) = &diagnosis.url {
        out.push_str(&format!("\n{url}\n"));
    }
    out
}

pub(crate) async fn handle_build_why(args: BuildWhyArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let build_id = args.build_id.trim();
    let tail = usize::try_from(args.tail).unwrap_or(40);
    let path = api.url_with(&["v1", "builds", build_id, "diagnosis"])?;
    let diagnosis = match api
        .send_if_supported::<BuildDiagnosis>(
            api.client.get(path).query(&[("tail", tail.to_string())]),
            "build diagnosis",
        )
        .await
    {
        Ok(Some(diagnosis)) => diagnosis,
        // A 404 here is a missing route or a missing build; the build
        // lookup below tells which.
        Ok(None) => diagnose_from_parts(&api, build_id, tail).await?,
        Err(error) => return Err(error),
    };
    if args.json {
        print_json(&diagnosis)
    } else {
        print!("{}", render_diagnosis(&diagnosis));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(sequence: i64, content: &str) -> BuildLogChunk {
        BuildLogChunk {
            sequence,
            content: content.to_string(),
            stream: "stdout".to_string(),
        }
    }

    fn start(name: &str) -> String {
        format!(
            "{STEP_MARKER}{}",
            serde_json::json!({ "event": "start", "name": name, "command": "x" })
        )
    }

    fn end(name: &str, status: &str, code: i64) -> String {
        format!(
            "{STEP_MARKER}{}",
            serde_json::json!({ "event": "end", "name": name, "status": status, "exit_code": code })
        )
    }

    #[test]
    fn sse_events_split_across_chunks() {
        let mut parser = SseParser::default();
        assert!(parser.push(b"event: log\nid: 4\ndata: {\"a\"").is_empty());
        let events = parser.push(b":1}\n\n: keep-alive\n\nevent: done\ndata: build_finished\n\n");
        assert_eq!(
            events,
            vec![
                SseEvent {
                    event: "log".into(),
                    data: "{\"a\":1}".into(),
                    id: Some("4".into()),
                },
                SseEvent {
                    event: "done".into(),
                    data: "build_finished".into(),
                    id: None,
                },
            ]
        );
    }

    #[test]
    fn markers_name_each_line_step_and_filters_apply() {
        let chunks = vec![
            chunk(1, &format!("{}\nResolving dependencies", start("Install"))),
            chunk(2, &end("Install", "succeeded", 0)),
            chunk(3, &start("Build Android")),
            chunk(4, "Compiling\nerror: boom\nFAILURE"),
            chunk(5, &end("Build Android", "failed", 1)),
        ];
        let mut current = None;
        let lines = log_lines(&chunks, &mut current);
        assert_eq!(lines[0].text, "==> Install started");
        assert_eq!(lines[1].step.as_deref(), Some("Install"));
        assert_eq!(lines[5].step.as_deref(), Some("Build Android"));
        assert_eq!(
            lines.last().unwrap().text,
            "==> Build Android failed (exit 1)"
        );

        let step = LogFilter {
            step: Some("build android".into()),
            ..LogFilter::default()
        };
        let (kept, truncated) = step.apply(lines.clone());
        assert_eq!(kept.len(), 5);
        assert!(!truncated);

        let search = LogFilter {
            search: Some("ERROR".into()),
            tail: Some(1),
            ..LogFilter::default()
        };
        let (kept, _) = search.apply(lines.clone());
        assert_eq!(
            kept.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(),
            ["error: boom"]
        );

        let tail = LogFilter {
            tail: Some(2),
            ..LogFilter::default()
        };
        let (kept, truncated) = tail.apply(lines);
        assert_eq!(kept.len(), 2);
        assert!(truncated);
    }

    #[test]
    fn a_diagnosis_reads_cause_fix_tests_and_tail() {
        let diagnosis: BuildDiagnosis = serde_json::from_value(serde_json::json!({
            "build_id": "b1",
            "build_number": 12,
            "status": "failed",
            "failed_step": {"name": "build-android", "label": "Build Android", "exit_code": 1},
            "hint": {"line": "No space left on device", "sequence": 9, "category": "disk",
                     "cause": "The runner ran out of disk space.", "fix": "Free space on the runner."},
            "log_tail": ["Compiling", "No space left on device"],
            "log_tail_truncated": true,
            "failed_tests": [{"name": "login works", "message": "Expected true\nstack"}],
            "failed_tests_total": 3,
            "url": "https://oore.example/builds/b1"
        }))
        .unwrap();
        let text = render_diagnosis(&diagnosis);
        assert!(text.starts_with("Build #12 failed in step Build Android (exit 1)\n"));
        assert!(text.contains("Cause: The runner ran out of disk space.\nFix: Free space"));
        assert!(text.contains("3 failed tests:\n  login works: Expected true\n"));
        assert!(text.contains("Last 2 lines of the step:\n  Compiling\n"));
        assert!(text.ends_with("https://oore.example/builds/b1\n"));

        let mut ok = diagnosis;
        ok.status = "succeeded".into();
        assert_eq!(
            render_diagnosis(&ok),
            "Build #12 succeeded; it did not fail, so there is nothing to explain.\n"
        );
    }
}
