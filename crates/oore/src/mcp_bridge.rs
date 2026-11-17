//! `oore mcp`: a stdio bridge to the daemon's MCP endpoint, for agent
//! clients that can only start a local command. Each JSON-RPC message read
//! from standard input is posted to `<daemon>/mcp` with the CLI's sign-in,
//! and every answer is written to standard output, one per line. Nothing
//! else goes to standard output; diagnostics go to standard error.

use std::collections::HashSet;

use anyhow::Context;
use clap::Args;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::build_commands::{Api, ApiConnectionArgs};

const MCP_OFF: &str = "MCP is turned off on this Oore instance. An owner turns it on in Settings > General > MCP, then agents can connect.";

#[derive(Debug, Args)]
pub(crate) struct McpArgs {
    /// Offer only tools marked read-only, and refuse calls to any other.
    #[arg(long, default_value = "false")]
    read_only: bool,

    #[command(flatten)]
    connection: ApiConnectionArgs,
}

/// State kept between messages.
struct Bridge {
    api: Api,
    endpoint: url::Url,
    tools: ToolFilter,
    protocol_version: Option<String>,
    session_id: Option<String>,
}

/// What `--read-only` needs to know about tools.
struct ToolFilter {
    read_only: bool,
    /// Read-only tool names, once a tool list has been seen.
    read_only_tools: Option<HashSet<String>>,
}

/// What one POST answered.
enum Reply {
    Messages(Vec<Value>),
    /// The instance has MCP turned off.
    Off,
}

pub(crate) async fn handle(args: McpArgs) -> anyhow::Result<()> {
    let api = Api::connect(&args.connection)?;
    let mut endpoint = url::Url::parse(&api.daemon_url).context("invalid daemon URL")?;
    endpoint
        .path_segments_mut()
        .map_err(|()| anyhow::anyhow!("invalid daemon URL"))?
        .pop_if_empty()
        .push("mcp");
    let mut bridge = Bridge {
        api,
        endpoint,
        tools: ToolFilter {
            read_only: args.read_only,
            read_only_tools: None,
        },
        protocol_version: None,
        session_id: None,
    };
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    while let Some(line) = lines
        .next_line()
        .await
        .context("failed to read standard input")?
    {
        if line.trim().is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(&line) {
            Ok(message) => message,
            Err(error) => {
                write_message(
                    &mut stdout,
                    &rpc_error(Value::Null, -32700, &format!("parse error: {error}")),
                )
                .await?;
                continue;
            }
        };
        for answer in bridge.forward(message).await? {
            write_message(&mut stdout, &answer).await?;
        }
    }
    Ok(())
}

async fn write_message(stdout: &mut tokio::io::Stdout, message: &Value) -> anyhow::Result<()> {
    let mut line = serde_json::to_string(message)?;
    line.push('\n');
    stdout.write_all(line.as_bytes()).await?;
    stdout.flush().await?;
    Ok(())
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// The `id` of a request, or `None` for a notification.
fn request_id(message: &Value) -> Option<Value> {
    message.get("id").filter(|id| !id.is_null()).cloned()
}

impl Bridge {
    async fn forward(&mut self, message: Value) -> anyhow::Result<Vec<Value>> {
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .map(str::to_string);
        if method.as_deref() == Some("initialize")
            && let Some(version) = message
                .pointer("/params/protocolVersion")
                .and_then(Value::as_str)
        {
            self.protocol_version = Some(version.to_string());
        }
        if self.tools.read_only
            && method.as_deref() == Some("tools/call")
            && let Some(id) = request_id(&message)
        {
            let tool = message
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if !self.tool_is_read_only(&tool).await? {
                return Ok(vec![rpc_error(
                    id,
                    -32602,
                    &format!(
                        "{tool} is not a read-only tool, and this bridge runs with --read-only"
                    ),
                )]);
            }
        }
        let messages = match self.post(&message).await? {
            Reply::Messages(messages) => messages,
            Reply::Off => {
                eprintln!("{MCP_OFF}");
                let mut answers = Vec::new();
                if let Some(id) = request_id(&message) {
                    answers.push(rpc_error(id, -32001, MCP_OFF));
                }
                return Ok(answers);
            }
        };
        if method.as_deref() == Some("tools/list") {
            return Ok(messages
                .into_iter()
                .map(|message| self.tools.filter(message))
                .collect());
        }
        Ok(messages)
    }

    async fn post(&mut self, message: &Value) -> anyhow::Result<Reply> {
        let mut request = self
            .api
            .client
            .post(self.endpoint.clone())
            .bearer_auth(self.api.token())
            .header(
                reqwest::header::ACCEPT,
                "application/json, text/event-stream",
            )
            .json(message);
        if let Some(version) = &self.protocol_version {
            request = request.header("MCP-Protocol-Version", version);
        }
        if let Some(session) = &self.session_id {
            request = request.header("Mcp-Session-Id", session);
        }
        let response = request
            .send()
            .await
            .with_context(|| format!("failed to reach daemon at {}", self.api.daemon_url))?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(Reply::Off);
        }
        if let Some(session) = response
            .headers()
            .get("Mcp-Session-Id")
            .and_then(|value| value.to_str().ok())
        {
            self.session_id = Some(session.to_string());
        }
        let event_stream = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("text/event-stream"));
        let body = response
            .text()
            .await
            .context("failed to read the MCP response")?;
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            anyhow::bail!(
                "the daemon refused the MCP request (HTTP {}): sign in with `oore login`, or pass --token or OORE_TOKEN",
                status.as_u16()
            );
        }
        if body.trim().is_empty() {
            // 202 for a notification or a response the client sent.
            return Ok(Reply::Messages(Vec::new()));
        }
        let messages = if event_stream {
            parse_event_stream(&body)
        } else {
            parse_json_body(&body)
        };
        if messages.is_empty() && !status.is_success() {
            let id = request_id(message).unwrap_or(Value::Null);
            return Ok(Reply::Messages(vec![rpc_error(
                id,
                -32603,
                &format!(
                    "the daemon answered HTTP {}: {}",
                    status.as_u16(),
                    body.trim()
                ),
            )]));
        }
        Ok(Reply::Messages(messages))
    }

    /// Whether `tool` is marked read-only, listing tools once if needed.
    async fn tool_is_read_only(&mut self, tool: &str) -> anyhow::Result<bool> {
        if self.tools.read_only_tools.is_none() {
            let list =
                json!({ "jsonrpc": "2.0", "id": "oore-mcp-read-only", "method": "tools/list" });
            if let Reply::Messages(messages) = self.post(&list).await? {
                for message in messages {
                    self.tools.filter(message);
                }
            }
        }
        Ok(self
            .tools
            .read_only_tools
            .as_ref()
            .is_some_and(|tools| tools.contains(tool)))
    }
}

impl ToolFilter {
    /// Remembers the read-only tools, and with `--read-only` drops the
    /// others from the list.
    fn filter(&mut self, mut message: Value) -> Value {
        let Some(tools) = message
            .pointer_mut("/result/tools")
            .and_then(Value::as_array_mut)
        else {
            return message;
        };
        let read_only = |tool: &Value| {
            tool.pointer("/annotations/readOnlyHint")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        };
        let names = self.read_only_tools.get_or_insert_with(HashSet::new);
        for tool in tools.iter().filter(|tool| read_only(tool)) {
            if let Some(name) = tool.get("name").and_then(Value::as_str) {
                names.insert(name.to_string());
            }
        }
        if self.read_only {
            tools.retain(read_only);
        }
        message
    }
}

fn parse_json_body(body: &str) -> Vec<Value> {
    match serde_json::from_str::<Value>(body) {
        Ok(Value::Array(batch)) => batch,
        Ok(message) => vec![message],
        Err(_) => Vec::new(),
    }
}

/// The JSON messages in a `text/event-stream` body: each event's `data`
/// lines joined.
fn parse_event_stream(body: &str) -> Vec<Value> {
    let mut messages = Vec::new();
    let mut data = String::new();
    let flush = |data: &mut String, messages: &mut Vec<Value>| {
        if !data.is_empty() {
            messages.extend(parse_json_body(data));
            data.clear();
        }
    };
    for line in body.lines() {
        if line.is_empty() {
            flush(&mut data, &mut messages);
        } else if let Some(value) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(value.strip_prefix(' ').unwrap_or(value));
        }
    }
    flush(&mut data, &mut messages);
    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_streams_yield_each_message() {
        let body = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n: ping\n\ndata: {\"jsonrpc\":\"2.0\",\ndata: \"method\":\"notifications/progress\"}\n";
        let messages = parse_event_stream(body);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["id"], 1);
        assert_eq!(messages[1]["method"], "notifications/progress");
    }

    fn bridge(read_only: bool) -> ToolFilter {
        ToolFilter {
            read_only,
            read_only_tools: None,
        }
    }

    #[test]
    fn read_only_hides_tools_that_change_things() {
        let list = json!({ "jsonrpc": "2.0", "id": 1, "result": { "tools": [
            { "name": "get_build", "annotations": { "readOnlyHint": true } },
            { "name": "run_build", "annotations": { "readOnlyHint": false } },
            { "name": "cancel_build" }
        ]}});
        let mut open = bridge(false);
        assert_eq!(
            open.filter(list.clone())["result"]["tools"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        let mut locked = bridge(true);
        let filtered = locked.filter(list);
        let tools = filtered["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "get_build");
        assert!(locked.read_only_tools.unwrap().contains("get_build"));
    }
}
