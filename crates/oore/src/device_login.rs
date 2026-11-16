//! `oore login` against a remote instance: the device flow. The CLI asks
//! the daemon for a short code, a signed-in person approves it in the web
//! app, and the CLI receives an API token named for this device and saves
//! it like `oore login --token` does.

use std::io::IsTerminal;
use std::time::{Duration, Instant};

use anyhow::Context;
use serde_json::{Value, json};

use crate::{load_cli_config, open_browser, save_cli_config};

/// What `POST /v1/auth/device/start` answers.
#[derive(Debug, serde::Deserialize)]
struct DeviceStart {
    device_code: String,
    user_code: String,
    verification_url: String,
    /// The URL with the code filled in, when the daemon offers one.
    #[serde(default)]
    verification_url_complete: Option<String>,
    expires_in: u64,
    #[serde(default = "default_interval")]
    interval: u64,
}

fn default_interval() -> u64 {
    5
}

/// One answer to a poll.
#[derive(Debug, PartialEq)]
enum Poll {
    Token(String),
    Pending,
    SlowDown,
    Denied,
    Expired,
    Failed(String),
}

/// The name the token gets: "oore CLI on <hostname>".
fn device_name() -> String {
    let mut buffer = [0u8; 256];
    // SAFETY: the buffer is valid for its length, and gethostname writes at
    // most that many bytes.
    let ok = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) } == 0;
    let host = ok
        .then(|| {
            let end = buffer
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(buffer.len());
            String::from_utf8_lossy(&buffer[..end]).trim().to_string()
        })
        .filter(|host| !host.is_empty())
        .unwrap_or_else(|| "this device".to_string());
    format!("oore CLI on {host}")
}

/// Reads a poll answer, whether the daemon uses its `ApiError` shape
/// (`code`) or the OAuth device-flow shape (`error`).
fn read_poll(status: u16, body: &Value) -> Poll {
    if (200..300).contains(&status)
        && let Some(token) = ["token", "access_token", "session_token"]
            .iter()
            .find_map(|key| body.get(*key).and_then(Value::as_str))
    {
        return Poll::Token(token.to_string());
    }
    let code = body
        .get("code")
        .and_then(Value::as_str)
        .or_else(|| body.get("error").and_then(Value::as_str))
        .unwrap_or_default();
    match code {
        "authorization_pending" => Poll::Pending,
        "slow_down" => Poll::SlowDown,
        "access_denied" => Poll::Denied,
        "expired_token" => Poll::Expired,
        _ => Poll::Failed(
            body.get("error")
                .and_then(Value::as_str)
                .filter(|message| *message != code || code.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("HTTP {status}: {body}")),
        ),
    }
}

async fn post(client: &reqwest::Client, url: String, body: &Value) -> anyhow::Result<(u16, Value)> {
    let response = client
        .post(&url)
        .json(body)
        .send()
        .await
        .with_context(|| format!("failed to reach {url}"))?;
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    let body = serde_json::from_str(&text).unwrap_or_else(|_| json!({ "error": text.trim() }));
    Ok((status, body))
}

/// Whether the daemon offers the device flow at all.
pub(crate) async fn supported(client: &reqwest::Client, daemon_url: &str) -> bool {
    let url = format!("{}/v1/meta", daemon_url.trim_end_matches('/'));
    let Ok(response) = client.get(&url).send().await else {
        return false;
    };
    let Ok(meta) = response.json::<Value>().await else {
        return false;
    };
    meta["features"]
        .as_array()
        .is_some_and(|features| features.iter().any(|feature| feature == "device_login"))
}

/// Runs the device flow and saves the token. With `json` or no terminal,
/// prints the code and URL as one JSON line on stdout, then the result.
pub(crate) async fn login(
    client: &reqwest::Client,
    daemon_url: &str,
    json_output: bool,
) -> anyhow::Result<()> {
    let base = daemon_url.trim_end_matches('/');
    let name = device_name();
    let (status, body) = post(
        client,
        format!("{base}/v1/auth/device/start"),
        &json!({ "client_name": name }),
    )
    .await?;
    if status == 404 {
        anyhow::bail!(
            "{daemon_url} does not offer sign-in from another device yet. Create an API token in Settings > API tokens and run `oore login --token <token>`."
        );
    }
    anyhow::ensure!(
        (200..300).contains(&status),
        "device sign-in failed to start (HTTP {status}): {body}"
    );
    let start: DeviceStart =
        serde_json::from_value(body).context("unexpected device sign-in response")?;
    let open_url = start
        .verification_url_complete
        .clone()
        .unwrap_or_else(|| start.verification_url.clone());
    let interactive = !json_output && std::io::stderr().is_terminal();
    if interactive {
        eprintln!(
            "To sign in, open {}\nand approve code {}.",
            start.verification_url, start.user_code
        );
        if open_browser(&open_url) {
            eprintln!("Opened your browser. Waiting for approval...");
        } else {
            eprintln!("Waiting for approval...");
        }
    } else {
        println!(
            "{}",
            json!({
                "status": "waiting_for_approval",
                "user_code": start.user_code,
                "verification_url": start.verification_url,
                "verification_url_complete": start.verification_url_complete,
                "expires_in": start.expires_in,
            })
        );
    }

    let deadline = Instant::now() + Duration::from_secs(start.expires_in.max(1));
    let mut interval = Duration::from_secs(start.interval.clamp(1, 60));
    let token = loop {
        if Instant::now() >= deadline {
            anyhow::bail!("the code expired before anyone approved it; run `oore login` again");
        }
        tokio::time::sleep(interval).await;
        let (status, body) = post(
            client,
            format!("{base}/v1/auth/device/token"),
            &json!({ "device_code": start.device_code }),
        )
        .await?;
        match read_poll(status, &body) {
            Poll::Token(token) => break token,
            Poll::Pending => {}
            Poll::SlowDown => interval += Duration::from_secs(5),
            Poll::Denied => anyhow::bail!("the sign-in was denied in the browser"),
            Poll::Expired => {
                anyhow::bail!("the code expired before anyone approved it; run `oore login` again")
            }
            Poll::Failed(message) => anyhow::bail!("device sign-in failed: {message}"),
        }
    };

    let profile = crate::fetch_user_profile(client, daemon_url, &token)
        .await
        .context("the new token was rejected")?;
    let mut cfg = load_cli_config()?;
    cfg.daemon_url = Some(daemon_url.to_string());
    cfg.session_token = Some(token);
    let config_path = save_cli_config(&cfg)?;
    if interactive {
        println!("Signed in as {} on {daemon_url}.", profile.user.email);
        println!("The token is named \"{name}\" and can be revoked in Settings > API tokens.");
        println!("Config: {}", config_path.display());
    } else {
        println!(
            "{}",
            json!({
                "ok": true,
                "auth_mode": "device",
                "daemon_url": daemon_url,
                "config_path": config_path,
                "token_name": name,
                "user": profile.user,
            })
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polls_read_both_error_shapes() {
        assert_eq!(
            read_poll(200, &json!({ "token": "oore_abc" })),
            Poll::Token("oore_abc".into())
        );
        assert_eq!(
            read_poll(
                400,
                &json!({ "code": "authorization_pending", "error": "Waiting" })
            ),
            Poll::Pending
        );
        assert_eq!(
            read_poll(400, &json!({ "error": "slow_down" })),
            Poll::SlowDown
        );
        assert_eq!(
            read_poll(400, &json!({ "error": "access_denied" })),
            Poll::Denied
        );
        assert_eq!(
            read_poll(400, &json!({ "code": "expired_token", "error": "Expired" })),
            Poll::Expired
        );
        assert!(matches!(
            read_poll(500, &json!({ "code": "internal", "error": "boom" })),
            Poll::Failed(message) if message == "boom"
        ));
    }

    #[test]
    fn the_token_is_named_for_the_device() {
        assert!(device_name().starts_with("oore CLI on "));
    }
}
