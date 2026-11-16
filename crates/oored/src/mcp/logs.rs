//! Reads a build's log and cuts it down to what an agent asked for: one
//! step, lines that match a search, the last lines, within a size cap.

use serde_json::{Value, json};

use super::client::{ApiClient, ApiFailure};

const STEP_MARKER: &str = "[oore-step] ";
/// The API answers at most this many log rows per call.
const PAGE: usize = 5_000;
/// A build keeps at most about 10,000 log rows, so this bounds the reads.
const MAX_PAGES: usize = 4;
/// One line is cut to this many characters.
const MAX_LINE_CHARS: usize = 400;
/// All returned lines together stay under this many characters.
const MAX_TOTAL_CHARS: usize = 24_000;

pub const DEFAULT_TAIL: usize = 100;
pub const MAX_TAIL: usize = 500;

/// One log line, with the step it belongs to.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub sequence: i64,
    pub step: Option<String>,
    pub text: String,
}

/// A step boundary written by the runner.
#[derive(Debug, PartialEq)]
enum Marker {
    Start(String),
    End {
        name: String,
        status: Option<String>,
        exit_code: Option<i64>,
    },
}

fn marker(line: &str) -> Option<Marker> {
    let json = line.trim().strip_prefix(STEP_MARKER)?;
    let value: Value = serde_json::from_str(json).ok()?;
    let name = value.get("name")?.as_str()?.to_string();
    match value.get("event")?.as_str()? {
        "start" => Some(Marker::Start(name)),
        "end" => Some(Marker::End {
            name,
            status: value
                .get("status")
                .and_then(Value::as_str)
                .map(str::to_string),
            exit_code: value.get("exit_code").and_then(Value::as_i64),
        }),
        _ => None,
    }
}

/// Reads the whole stored log of a build, oldest first, with step names.
pub async fn read_all(api: &ApiClient, build_id: &str) -> Result<Vec<Line>, ApiFailure> {
    let mut raw: Vec<(i64, String)> = Vec::new();
    let mut after = -1i64;
    for _ in 0..MAX_PAGES {
        let page = api
            .get(&format!(
                "/v1/builds/{}/logs?after_sequence={after}&limit={PAGE}",
                urlencoding::encode(build_id)
            ))
            .await?;
        let chunks = page
            .get("logs")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let count = chunks.len();
        for chunk in chunks {
            let Some(sequence) = chunk.get("sequence").and_then(Value::as_i64) else {
                continue;
            };
            let content = chunk
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default();
            after = after.max(sequence);
            raw.push((sequence, content.to_string()));
        }
        if count < PAGE {
            break;
        }
    }
    Ok(with_steps(raw))
}

/// Gives each line the step it was written in. Marker lines become short
/// readable lines.
pub fn with_steps(raw: Vec<(i64, String)>) -> Vec<Line> {
    let mut current: Option<String> = None;
    let mut lines = Vec::with_capacity(raw.len());
    for (sequence, content) in raw {
        for text in content.split('\n') {
            let text = text.trim_end_matches('\r');
            match marker(text) {
                Some(Marker::Start(name)) => {
                    lines.push(Line {
                        sequence,
                        step: Some(name.clone()),
                        text: format!("== step started: {name}"),
                    });
                    current = Some(name);
                }
                Some(Marker::End {
                    name,
                    status,
                    exit_code,
                }) => {
                    let mut text = format!("== step ended: {name}");
                    if let Some(status) = status {
                        text.push_str(&format!(", {status}"));
                    }
                    if let Some(code) = exit_code {
                        text.push_str(&format!(", exit code {code}"));
                    }
                    lines.push(Line {
                        sequence,
                        step: Some(name),
                        text,
                    });
                    current = None;
                }
                None => lines.push(Line {
                    sequence,
                    step: current.clone(),
                    text: text.to_string(),
                }),
            }
        }
    }
    lines
}

/// What to keep from a log.
#[derive(Debug, Default, Clone)]
pub struct Filter {
    pub step: Option<String>,
    pub search: Option<String>,
    pub tail: Option<usize>,
    pub after_sequence: Option<i64>,
}

/// Cuts a log down and shapes it as a tool result.
pub fn select(lines: &[Line], filter: &Filter) -> Value {
    let mut steps: Vec<&str> = Vec::new();
    for line in lines {
        if let Some(step) = line.step.as_deref()
            && !steps.contains(&step)
        {
            steps.push(step);
        }
    }
    let step = filter
        .step
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let search = filter
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase);

    let matching: Vec<&Line> = lines
        .iter()
        .filter(|line| {
            filter
                .after_sequence
                .is_none_or(|after| line.sequence > after)
        })
        .filter(|line| {
            step.is_none_or(|wanted| {
                line.step
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case(wanted))
            })
        })
        .filter(|line| {
            search
                .as_deref()
                .is_none_or(|needle| line.text.to_lowercase().contains(needle))
        })
        .collect();

    let tail = filter.tail.unwrap_or(DEFAULT_TAIL).clamp(1, MAX_TAIL);
    let start = matching.len().saturating_sub(tail);
    let mut kept: Vec<&Line> = matching[start..].to_vec();
    let mut cut_lines = false;
    let mut size = 0usize;
    let mut from = kept.len();
    // Keep the newest lines that fit in the size cap.
    while from > 0 {
        let len = kept[from - 1].text.chars().count().min(MAX_LINE_CHARS) + 12;
        if size + len > MAX_TOTAL_CHARS {
            break;
        }
        size += len;
        from -= 1;
    }
    if from > 0 {
        kept.drain(..from);
    }
    let rendered: Vec<String> = kept
        .iter()
        .map(|line| {
            let mut text: String = line.text.chars().take(MAX_LINE_CHARS).collect();
            if line.text.chars().count() > MAX_LINE_CHARS {
                text.push_str(" [line cut]");
                cut_lines = true;
            }
            format!("{} {}", line.sequence, text)
        })
        .collect();

    let truncated = rendered.len() < matching.len();
    let mut result = json!({
        "lines": rendered,
        "returned": rendered.len(),
        "matching_lines": matching.len(),
        "truncated": truncated,
        "steps": steps,
    });
    if let Some(last) = kept.last() {
        result["last_sequence"] = json!(last.sequence);
    }
    if let Some(first) = kept.first() {
        result["first_sequence"] = json!(first.sequence);
    }
    if step.is_some() && matching.is_empty() && !steps.is_empty() {
        result["note"] = json!("No lines for that step. `steps` lists the steps in this log.");
    } else if truncated {
        result["note"] = json!(format!(
            "Showing the last {} of {} matching lines. Each line starts with its sequence; \
             narrow with step or search, or pass after_sequence to read on.",
            rendered.len(),
            matching.len()
        ));
    }
    if cut_lines {
        result["lines_cut_at_chars"] = json!(MAX_LINE_CHARS);
    }
    result
}

/// The last `tail` lines of one step, for a diagnosis.
pub fn step_tail(lines: &[Line], step: Option<&str>, tail: usize) -> (Vec<String>, bool) {
    let matching: Vec<&Line> = lines
        .iter()
        .filter(|line| step.is_none_or(|name| line.step.as_deref() == Some(name)))
        .collect();
    let start = matching.len().saturating_sub(tail);
    let rendered = matching[start..]
        .iter()
        .map(|line| line.text.chars().take(MAX_LINE_CHARS).collect())
        .collect();
    (rendered, start > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start(name: &str) -> String {
        format!(
            "{STEP_MARKER}{}",
            json!({"event": "start", "name": name, "command": "x"})
        )
    }

    fn end(name: &str) -> String {
        format!(
            "{STEP_MARKER}{}",
            json!({"event": "end", "name": name, "status": "failed", "exit_code": 1})
        )
    }

    fn sample() -> Vec<Line> {
        let raw = vec![
            start("Install"),
            "fetching packages".to_string(),
            end("Install"),
            start("Build Android"),
            "Compiling".to_string(),
            "FAILURE: Build failed".to_string(),
            end("Build Android"),
        ];
        with_steps(
            raw.into_iter()
                .enumerate()
                .map(|(i, text)| (i as i64, text))
                .collect(),
        )
    }

    #[test]
    fn lines_carry_their_step_and_markers_read_as_words() {
        let lines = sample();
        assert_eq!(lines[1].step.as_deref(), Some("Install"));
        assert_eq!(lines[4].step.as_deref(), Some("Build Android"));
        assert_eq!(lines[0].text, "== step started: Install");
        assert_eq!(
            lines[6].text,
            "== step ended: Build Android, failed, exit code 1"
        );
    }

    #[test]
    fn step_and_search_filter_and_tail_keeps_the_newest() {
        let lines = sample();
        let result = select(
            &lines,
            &Filter {
                step: Some("build android".into()),
                ..Filter::default()
            },
        );
        assert_eq!(result["matching_lines"], 4);
        assert_eq!(result["steps"], json!(["Install", "Build Android"]));

        let result = select(
            &lines,
            &Filter {
                search: Some("failure".into()),
                ..Filter::default()
            },
        );
        assert_eq!(result["lines"], json!(["5 FAILURE: Build failed"]));

        let result = select(
            &lines,
            &Filter {
                tail: Some(2),
                ..Filter::default()
            },
        );
        assert_eq!(result["returned"], 2);
        assert_eq!(result["truncated"], true);
        assert_eq!(result["first_sequence"], 5);
    }

    #[test]
    fn a_huge_log_stays_under_the_size_cap() {
        let raw: Vec<(i64, String)> = (0..2_000).map(|i| (i, "x".repeat(1_000))).collect();
        let lines = with_steps(raw);
        let result = select(
            &lines,
            &Filter {
                tail: Some(MAX_TAIL),
                ..Filter::default()
            },
        );
        let text = result["lines"].to_string();
        assert!(text.len() < MAX_TOTAL_CHARS + 2_000, "{}", text.len());
        assert_eq!(result["truncated"], true);
        assert_eq!(result["lines_cut_at_chars"], MAX_LINE_CHARS);
        assert_eq!(result["last_sequence"], 1_999);
    }

    #[test]
    fn step_tail_reads_only_that_step() {
        let lines = sample();
        let (tail, cut) = step_tail(&lines, Some("Build Android"), 2);
        assert_eq!(
            tail,
            vec![
                "FAILURE: Build failed".to_string(),
                "== step ended: Build Android, failed, exit code 1".to_string()
            ]
        );
        assert!(cut);
    }
}
