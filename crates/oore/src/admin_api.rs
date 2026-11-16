//! Shared plumbing for the admin command families (`oore token`, `secret`,
//! `signing`, `tester`, `runner list`, `release`, `promote`): JSON calls to
//! the daemon, errors that keep the HTTP status, reading secret values
//! without putting them in arguments, and asking before destructive writes.

use std::io::{IsTerminal, Read};
use std::path::Path;

use anyhow::Context;
use oore_contract::ApiError;
use serde_json::Value;

use crate::build_commands::Api;
use crate::cli_contract::{CliError, exit};

/// A failed daemon call. Keeps the status and the API's code so the exit
/// code and `--json` error can be derived from it.
#[derive(Debug)]
pub(crate) struct ApiFailure {
    pub(crate) what: String,
    pub(crate) status: u16,
    pub(crate) code: Option<String>,
    pub(crate) message: String,
    pub(crate) hint: Option<String>,
}

impl std::fmt::Display for ApiFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} failed (HTTP {}): {}",
            self.what, self.status, self.message
        )?;
        if let Some(code) = &self.code {
            write!(f, " [{code}]")?;
        }
        if let Some(hint) = &self.hint {
            write!(f, "\n{hint}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ApiFailure {}

impl ApiFailure {
    /// The process exit code from the CLI's table: 4 not allowed, 5 not
    /// found, 1 otherwise.
    pub(crate) fn exit_code(&self) -> i32 {
        match self.status {
            401 | 403 => 4,
            404 => 5,
            _ => 1,
        }
    }
}

/// Adds a hint to a 403 naming what the caller needs.
pub(crate) fn with_forbidden_hint(error: anyhow::Error, hint: &str) -> anyhow::Error {
    match error.downcast::<ApiFailure>() {
        Ok(mut failure) if failure.status == 403 => {
            failure.hint = Some(hint.to_string());
            failure.into()
        }
        Ok(failure) => failure.into(),
        Err(error) => error,
    }
}

/// `<daemon>/v1/<segments...>` with each segment escaped.
pub(crate) fn api_url(api: &Api, segments: &[&str]) -> anyhow::Result<url::Url> {
    let mut url = url::Url::parse(&api.daemon_url).context("invalid daemon URL")?;
    url.path_segments_mut()
        .map_err(|()| anyhow::anyhow!("invalid daemon URL"))?
        .pop_if_empty()
        .push("v1")
        .extend(segments.iter().map(|segment| segment.trim()));
    Ok(url)
}

/// One call to the daemon. Answers `Value::Null` for an empty body.
pub(crate) async fn call(
    api: &Api,
    method: reqwest::Method,
    url: url::Url,
    body: Option<&Value>,
    what: &str,
) -> anyhow::Result<Value> {
    let mut request = api
        .client
        .request(method.clone(), url)
        .bearer_auth(api.token());
    if method != reqwest::Method::GET {
        // Replays of the same write are safe once the daemon keeps keys
        // (A2); until then the header is ignored.
        request = request.header("Idempotency-Key", uuid::Uuid::new_v4().to_string());
    }
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request
        .send()
        .await
        .with_context(|| format!("failed to reach daemon at {}", api.daemon_url))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .with_context(|| format!("failed to read the {what} response"))?;
    if status.is_success() {
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        return serde_json::from_str(&text)
            .with_context(|| format!("failed to parse the {what} response"));
    }
    let (message, code) = match serde_json::from_str::<ApiError>(&text) {
        Ok(error) => {
            let message = match error.details.as_deref().map(str::trim) {
                Some(details) if !details.is_empty() => format!("{}: {details}", error.error),
                _ => error.error,
            };
            (message, Some(error.code))
        }
        Err(_) if text.trim().is_empty() => (
            status
                .canonical_reason()
                .unwrap_or("request failed")
                .to_string(),
            None,
        ),
        Err(_) => (text.trim().to_string(), None),
    };
    Err(ApiFailure {
        what: what.to_string(),
        status: status.as_u16(),
        code,
        message,
        hint: None,
    }
    .into())
}

pub(crate) async fn get(api: &Api, url: url::Url, what: &str) -> anyhow::Result<Value> {
    call(api, reqwest::Method::GET, url, None, what).await
}

/// Decodes a response for the human output, keeping `--json` output as the
/// daemon sent it.
pub(crate) fn decode<T: serde::de::DeserializeOwned>(
    value: &Value,
    what: &str,
) -> anyhow::Result<T> {
    serde_json::from_value(value.clone()).with_context(|| format!("unexpected {what} response"))
}

pub(crate) fn print_json(value: &Value) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

/// `2026-11-16 20:44 UTC`, or `-` for none.
pub(crate) fn format_time(unix: Option<i64>) -> String {
    unix.and_then(|seconds| chrono::DateTime::from_timestamp(seconds, 0))
        .map(|time| time.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|| "-".to_string())
}

/// Prints rows as aligned columns under a header.
pub(crate) fn print_table(header: &[&str], rows: &[Vec<String>]) {
    let mut widths = header.iter().map(|cell| cell.len()).collect::<Vec<_>>();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if let Some(width) = widths.get_mut(index) {
                *width = (*width).max(cell.chars().count());
            }
        }
    }
    let line = |cells: Vec<String>| {
        let last = cells.len().saturating_sub(1);
        cells
            .into_iter()
            .enumerate()
            .map(|(index, cell)| {
                if index == last {
                    cell
                } else {
                    format!("{cell:<width$}", width = widths[index])
                }
            })
            .collect::<Vec<_>>()
            .join("  ")
    };
    println!(
        "{}",
        line(header.iter().map(|cell| cell.to_string()).collect())
    );
    for row in rows {
        println!("{}", line(row.clone()));
    }
}

/// Where a secret value comes from. Never from a command-line argument, so
/// it stays out of shell history and process lists.
#[derive(Debug, Clone, Copy)]
pub(crate) enum SecretSource<'a> {
    /// A file's contents.
    File(&'a Path),
    /// An environment variable's value.
    Env(&'a str),
    /// Standard input: a hidden prompt in a terminal, else everything piped
    /// in.
    Stdin,
}

pub(crate) fn read_secret(source: SecretSource<'_>, prompt: &str) -> anyhow::Result<String> {
    let value = match source {
        SecretSource::File(path) => std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?,
        SecretSource::Env(name) => std::env::var(name)
            .with_context(|| format!("environment variable {name} is not set"))?,
        SecretSource::Stdin => {
            if std::io::stdin().is_terminal() {
                let terminal = oore_cli_ui::Terminal::new(oore_cli_ui::ColorMode::Auto);
                match terminal.password(prompt, false)? {
                    oore_cli_ui::PromptResult::Submitted(value) => value,
                    oore_cli_ui::PromptResult::Cancelled => anyhow::bail!("cancelled"),
                }
            } else {
                let mut value = String::new();
                std::io::stdin()
                    .read_to_string(&mut value)
                    .context("failed to read standard input")?;
                value
            }
        }
    };
    // A trailing newline from `echo` or a file editor is never part of it.
    let value = value
        .strip_suffix("\r\n")
        .or_else(|| value.strip_suffix('\n'))
        .unwrap_or(&value)
        .to_string();
    anyhow::ensure!(!value.is_empty(), "the value is empty");
    Ok(value)
}

/// Asks before a destructive write. Without a terminal, `--yes` is required
/// so a script never blocks on a prompt.
pub(crate) fn confirm(yes: bool, question: &str) -> anyhow::Result<()> {
    if yes {
        return Ok(());
    }
    if !std::io::stdin().is_terminal() {
        return Err(UsageError(format!(
            "{question} Pass --yes to confirm when no terminal is attached."
        ))
        .into());
    }
    let terminal = oore_cli_ui::Terminal::new(oore_cli_ui::ColorMode::Auto);
    match terminal.confirm(question, false)? {
        oore_cli_ui::PromptResult::Submitted(true) => Ok(()),
        _ => anyhow::bail!("cancelled"),
    }
}

/// A mistake in how the command was called (exit code 64).
#[derive(Debug)]
pub(crate) struct UsageError(pub(crate) String);

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

#[cfg(test)]
/// The exit code for an error from these commands, per the CLI's table.
pub(crate) fn exit_code_for(error: &anyhow::Error) -> i32 {
    if let Some(failure) = error.downcast_ref::<ApiFailure>() {
        return failure.exit_code();
    }
    if error.downcast_ref::<UsageError>().is_some() {
        return 64;
    }
    1
}

/// The CLI's shared error for a failure from these commands, so `--json`
/// prints the error object and the exit code follows the table.
pub(crate) fn into_cli_error(error: anyhow::Error) -> anyhow::Error {
    if let Some(failure) = error.downcast_ref::<ApiFailure>() {
        let mut cli = CliError::new(
            u8::try_from(failure.exit_code()).unwrap_or(exit::FAILED),
            failure
                .code
                .clone()
                .unwrap_or_else(|| format!("http_{}", failure.status)),
            format!(
                "{} failed (HTTP {}): {}",
                failure.what, failure.status, failure.message
            ),
        );
        cli.status = Some(failure.status);
        if let Some(hint) = &failure.hint {
            cli = cli.with_hint(hint.clone());
        }
        return cli.into();
    }
    if let Some(usage) = error.downcast_ref::<UsageError>() {
        return CliError::usage(usage.0.clone()).into();
    }
    error
}

/// Runs an admin command on a fresh runtime, with its errors in the CLI's
/// shared shape.
pub(crate) fn run<F>(future: F) -> anyhow::Result<()>
where
    F: std::future::Future<Output = anyhow::Result<()>>,
{
    let runtime = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    runtime.block_on(future).map_err(into_cli_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failure(status: u16) -> anyhow::Error {
        ApiFailure {
            what: "x".into(),
            status,
            code: None,
            message: "no".into(),
            hint: None,
        }
        .into()
    }

    #[test]
    fn exit_codes_follow_the_table() {
        assert_eq!(exit_code_for(&failure(403)), 4);
        assert_eq!(exit_code_for(&failure(401)), 4);
        assert_eq!(exit_code_for(&failure(404)), 5);
        assert_eq!(exit_code_for(&failure(500)), 1);
        assert_eq!(exit_code_for(&UsageError("bad".into()).into()), 64);
        assert_eq!(exit_code_for(&anyhow::anyhow!("other")), 1);
    }

    #[test]
    fn failures_become_the_shared_cli_error() {
        let hinted = into_cli_error(with_forbidden_hint(failure(403), "needs manage"));
        let cli = hinted.downcast_ref::<CliError>().unwrap();
        assert_eq!(cli.exit, 4);
        assert_eq!(cli.status, Some(403));
        assert_eq!(cli.hint.as_deref(), Some("needs manage"));
        let usage = into_cli_error(UsageError("bad".into()).into());
        assert_eq!(usage.downcast_ref::<CliError>().unwrap().exit, 64);
    }

    #[test]
    fn a_forbidden_hint_only_lands_on_403() {
        let hinted = with_forbidden_hint(failure(403), "needs release_manage");
        assert!(hinted.to_string().contains("needs release_manage"));
        let other = with_forbidden_hint(failure(404), "needs release_manage");
        assert!(!other.to_string().contains("release_manage"));
    }

    #[test]
    fn a_secret_file_loses_only_its_trailing_newline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("value");
        std::fs::write(&path, "  s3cret \n").unwrap();
        assert_eq!(
            read_secret(SecretSource::File(&path), "").unwrap(),
            "  s3cret "
        );
        std::fs::write(&path, "\n").unwrap();
        assert!(read_secret(SecretSource::File(&path), "").is_err());
    }
}
