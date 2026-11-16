//! What every `oore` command promises scripts and agents: one exit code
//! table, one JSON error shape on stderr with `--json`, extra request
//! headers for every API call, and no prompt without a terminal.

use std::fmt;
use std::io::{IsTerminal, Write};
use std::sync::OnceLock;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

/// Exit codes shared by every command. `reference/cli/exit-codes` documents
/// them; keep the two in step.
pub(crate) mod exit {
    /// The command did what was asked.
    pub(crate) const OK: u8 = 0;
    /// The command failed, or the build it followed did not succeed.
    pub(crate) const FAILED: u8 = 1;
    /// Blocked until a person acts: readiness blocked, waiting for approval.
    pub(crate) const BLOCKED: u8 = 2;
    /// The store blocked, rejected or failed the release.
    pub(crate) const STORE_REFUSED: u8 = 3;
    /// Not signed in, or not allowed (HTTP 401 or 403).
    pub(crate) const NOT_ALLOWED: u8 = 4;
    /// The thing named does not exist (HTTP 404).
    pub(crate) const NOT_FOUND: u8 = 5;
    /// The command line was wrong. Clap's own default, 2, means "blocked" here.
    pub(crate) const USAGE: u8 = 64;
}

/// The exit code table as `--help` prints it.
pub(crate) const EXIT_CODES_HELP: &str = "\
Exit codes:
  0   ok
  1   failed, or the build did not succeed
  2   blocked until a person acts (readiness blocked, waiting for approval)
  3   the store blocked, rejected or failed the release
  4   not signed in or not allowed
  5   not found
  64  usage error

Connection: --url or OORE_URL, --token or OORE_TOKEN, and --header or
OORE_HTTP_HEADERS for extra request headers. --json prints JSON on stdout and,
on failure, one JSON error object on stderr.";

/// A failure that knows its exit code and, when it came from the API, the
/// API's error code and HTTP status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliError {
    pub(crate) message: String,
    pub(crate) code: String,
    pub(crate) status: Option<u16>,
    pub(crate) hint: Option<String>,
    pub(crate) exit: u8,
    /// The command already said what happened in text, so only `--json`
    /// prints this failure.
    pub(crate) quiet: bool,
}

impl CliError {
    pub(crate) fn new(exit: u8, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: code.into(),
            status: None,
            hint: None,
            exit,
            quiet: false,
        }
    }

    /// A result the command already printed in text, such as a build that
    /// failed; only `--json` prints it again as an error object.
    pub(crate) fn outcome(exit: u8, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            quiet: true,
            ..Self::new(exit, code, message)
        }
    }

    pub(crate) fn not_found(message: impl Into<String>) -> Self {
        Self::new(exit::NOT_FOUND, "not_found", message)
    }

    pub(crate) fn usage(message: impl Into<String>) -> Self {
        Self::new(exit::USAGE, "usage", message)
    }

    pub(crate) fn not_signed_in(message: impl Into<String>) -> Self {
        Self::new(exit::NOT_ALLOWED, "not_signed_in", message)
    }

    pub(crate) fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// An API answer that was not a success. `what` names the request, such
    /// as "build lookup".
    pub(crate) fn from_api(status: u16, what: &str, body: &str) -> Self {
        let parsed = serde_json::from_str::<serde_json::Value>(body).ok();
        let field = |name: &str| {
            parsed
                .as_ref()
                .and_then(|value| value.get(name))
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let code = field("code").unwrap_or_else(|| format!("http_{status}"));
        let mut message = match field("error") {
            Some(error) => error,
            None if body.trim().is_empty() || parsed.is_some() => format!("HTTP {status}"),
            None => body.trim().chars().take(500).collect(),
        };
        if let Some(details) = field("details") {
            message = format!("{message}: {details}");
        }
        Self {
            message: format!("{what} failed (HTTP {status}): {message}"),
            code,
            status: Some(status),
            hint: field("hint").or_else(|| default_hint(status).map(str::to_string)),
            exit: exit_for_status(status),
            quiet: false,
        }
    }

    pub(crate) fn to_json(&self) -> serde_json::Value {
        let mut value = serde_json::json!({
            "error": self.message,
            "code": self.code,
            "status": self.status,
            "exit_code": self.exit,
        });
        if let Some(hint) = &self.hint {
            value["hint"] = serde_json::Value::String(hint.clone());
        }
        value
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}

/// The exit code for a failed HTTP answer.
pub(crate) fn exit_for_status(status: u16) -> u8 {
    match status {
        401 | 403 => exit::NOT_ALLOWED,
        404 => exit::NOT_FOUND,
        _ => exit::FAILED,
    }
}

fn default_hint(status: u16) -> Option<&'static str> {
    match status {
        401 => Some("Sign in with `oore login`, or pass --token or OORE_TOKEN."),
        403 => Some("Your role or token does not allow this. Ask an owner or admin."),
        _ => None,
    }
}

/// Whether `--json` is on the command line, read before clap parses it so a
/// usage error can be reported as JSON too.
pub(crate) fn json_requested<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    args.into_iter()
        .skip(1)
        .take_while(|arg| arg.as_ref() != "--")
        .any(|arg| arg.as_ref() == "--json")
}

/// What a failed command prints and the code it exits with.
pub(crate) fn describe_failure(error: &anyhow::Error) -> CliError {
    if let Some(cli) = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<CliError>())
    {
        let mut described = cli.clone();
        // Context added on the way up reads better than the bare cause.
        let outer = error.to_string();
        if outer != cli.message {
            described.message = format!("{outer}: {}", cli.message);
        }
        return described;
    }
    let message = error
        .chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ");
    let mut described = CliError::new(exit::FAILED, "failed", message);
    if error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::NotConnected)
    }) {
        described.code = "no_terminal".to_string();
        described.exit = exit::USAGE;
        described.hint = Some(
            "No interactive terminal: pass the answer as a flag. See `--help` for the flags."
                .to_string(),
        );
    }
    described
}

/// Prints a failure, as one JSON object on stderr with `--json`, and returns
/// the exit code.
pub(crate) fn report_failure(error: &anyhow::Error, json: bool) -> u8 {
    // A reader such as `head` closed the pipe: it has what it wanted.
    if error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::BrokenPipe)
    }) {
        return exit::OK;
    }
    let failure = describe_failure(error);
    let mut stderr = std::io::stderr().lock();
    if json {
        let _ = writeln!(stderr, "{}", failure.to_json());
    } else if !failure.quiet {
        let _ = writeln!(stderr, "Error: {}", failure.message);
        if let Some(hint) = &failure.hint {
            let _ = writeln!(stderr, "Hint: {hint}");
        }
    }
    failure.exit
}

/// Prints a value as pretty JSON on stdout.
pub(crate) fn print_json<T: serde::Serialize + ?Sized>(value: &T) -> anyhow::Result<()> {
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, value)?;
    writeln!(stdout)?;
    Ok(())
}

/// Prints a value as one JSON line on stdout, for streams of events.
pub(crate) fn print_json_line<T: serde::Serialize + ?Sized>(value: &T) -> anyhow::Result<()> {
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer(&mut stdout, value)?;
    writeln!(stdout)?;
    stdout.flush()?;
    Ok(())
}

/// Runs `work` with stdout pointed at stderr, so a command that prints prose
/// as it goes keeps stdout for the JSON it prints at the end.
pub(crate) fn with_stdout_on_stderr<T>(enabled: bool, work: impl FnOnce() -> T) -> T {
    if !enabled {
        return work();
    }
    let _ = std::io::stdout().flush();
    // SAFETY: duplicating the process's own stdout descriptor.
    let saved = unsafe { libc::dup(libc::STDOUT_FILENO) };
    if saved < 0 {
        return work();
    }
    // SAFETY: both descriptors are the process's own open stdio.
    if unsafe { libc::dup2(libc::STDERR_FILENO, libc::STDOUT_FILENO) } < 0 {
        // SAFETY: `saved` was opened just above and is not used again.
        unsafe { libc::close(saved) };
        return work();
    }
    let result = work();
    let _ = std::io::stdout().flush();
    // SAFETY: `saved` is the stdout descriptor duplicated above.
    unsafe {
        libc::dup2(saved, libc::STDOUT_FILENO);
        libc::close(saved);
    }
    result
}

/// Fails when a command would delete or replace state with no terminal to
/// ask and no `--yes`.
pub(crate) fn require_yes_without_terminal(yes: bool, action: &str) -> anyhow::Result<()> {
    if yes || std::io::stdin().is_terminal() {
        return Ok(());
    }
    Err(CliError::usage(format!(
        "{action} needs confirmation and there is no terminal to ask; rerun with --yes"
    ))
    .with_hint("Pass --yes to confirm without a prompt.")
    .into())
}

static EXTRA_HEADERS: OnceLock<HeaderMap> = OnceLock::new();

/// Parses one `Name: value` header.
pub(crate) fn parse_header(line: &str) -> anyhow::Result<(HeaderName, HeaderValue)> {
    let (name, value) = line.split_once(':').ok_or_else(|| {
        CliError::usage(format!("header '{line}' is not in the form 'Name: value'"))
    })?;
    let name = HeaderName::from_bytes(name.trim().as_bytes())
        .map_err(|_| CliError::usage(format!("'{}' is not a valid header name", name.trim())))?;
    let mut value = HeaderValue::from_str(value.trim())
        .map_err(|_| CliError::usage(format!("the value of header '{name}' is not valid")))?;
    value.set_sensitive(true);
    Ok((name, value))
}

/// The headers from `OORE_HTTP_HEADERS` (one `Name: value` per line) and then
/// each `--header`; a later header with the same name wins.
pub(crate) fn collect_headers(
    environment: Option<&str>,
    flags: &[String],
) -> anyhow::Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    let lines = environment
        .into_iter()
        .flat_map(str::lines)
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .chain(flags.iter().map(|flag| flag.trim()));
    for line in lines {
        let (name, value) = parse_header(line)?;
        headers.insert(name, value);
    }
    Ok(headers)
}

/// Sets the extra headers every API request sends. Called once from `main`.
pub(crate) fn init_extra_headers(flags: &[String]) -> anyhow::Result<()> {
    let environment = std::env::var("OORE_HTTP_HEADERS").ok();
    let headers = collect_headers(environment.as_deref(), flags)?;
    let _ = EXTRA_HEADERS.set(headers);
    Ok(())
}

/// The extra headers for API requests; empty until `main` sets them.
pub(crate) fn extra_headers() -> HeaderMap {
    EXTRA_HEADERS.get().cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_failures_keep_the_api_code_and_map_the_exit_code() {
        let error = CliError::from_api(
            404,
            "build lookup",
            r#"{"error":"Build not found","code":"build_not_found"}"#,
        );
        assert_eq!(error.exit, exit::NOT_FOUND);
        assert_eq!(error.code, "build_not_found");
        assert_eq!(error.status, Some(404));
        assert_eq!(
            error.message,
            "build lookup failed (HTTP 404): Build not found"
        );
        let json = error.to_json();
        assert_eq!(json["code"], "build_not_found");
        assert_eq!(json["status"], 404);
        assert!(json.get("hint").is_none());

        let error = CliError::from_api(401, "build lookup", "");
        assert_eq!(error.exit, exit::NOT_ALLOWED);
        assert_eq!(error.code, "http_401");
        assert!(error.hint.as_deref().unwrap().contains("oore login"));

        let error = CliError::from_api(
            403,
            "cancel",
            r#"{"error":"Forbidden","code":"forbidden","hint":"Ask a maintainer."}"#,
        );
        assert_eq!(error.exit, exit::NOT_ALLOWED);
        assert_eq!(error.hint.as_deref(), Some("Ask a maintainer."));

        assert_eq!(CliError::from_api(500, "x", "boom").exit, exit::FAILED);
        assert_eq!(
            CliError::from_api(500, "x", "boom").message,
            "x failed (HTTP 500): boom"
        );
    }

    #[test]
    fn failures_keep_their_exit_code_through_context() {
        use anyhow::Context;
        let error: anyhow::Result<()> = Err(CliError::not_found("no app named coin").into());
        let error = error.context("resolving --project").unwrap_err();
        let failure = describe_failure(&error);
        assert_eq!(failure.exit, exit::NOT_FOUND);
        assert_eq!(failure.message, "resolving --project: no app named coin");

        let plain = anyhow::anyhow!("disk full");
        assert_eq!(describe_failure(&plain).exit, exit::FAILED);
        assert_eq!(describe_failure(&plain).code, "failed");

        let no_terminal =
            anyhow::Error::new(std::io::Error::from(std::io::ErrorKind::NotConnected))
                .context("failed to read the device profile");
        let failure = describe_failure(&no_terminal);
        assert_eq!(failure.exit, exit::USAGE);
        assert_eq!(failure.code, "no_terminal");
    }

    #[test]
    fn json_is_found_before_the_argument_separator_only() {
        assert!(json_requested(["oore", "build", "list", "--json"]));
        assert!(!json_requested(["oore", "build", "list"]));
        assert!(!json_requested(["oore", "api", "x", "--", "--json"]));
        // The program name is never a flag.
        assert!(!json_requested(["--json"]));
    }

    #[test]
    fn headers_come_from_the_environment_then_flags() {
        let headers = collect_headers(
            Some("CF-Access-Client-Id: abc\n\n X-Team : mobile \n"),
            &[
                "CF-Access-Client-Secret: s3cret".to_string(),
                "x-team: web".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(headers["cf-access-client-id"], "abc");
        assert_eq!(headers["cf-access-client-secret"], "s3cret");
        assert_eq!(headers["x-team"], "web");
        assert!(headers["cf-access-client-secret"].is_sensitive());

        let error = collect_headers(None, &["no colon".to_string()]).unwrap_err();
        assert_eq!(describe_failure(&error).exit, exit::USAGE);
        assert!(collect_headers(None, &["bad name: x".to_string()]).is_err());
    }
}
