//! `oore api`: call any operation of the Oore API, by method and path or by
//! operation ID, with the saved login, like `gh api`.

use std::collections::BTreeMap;

use anyhow::Context;
use clap::Args;

use crate::build_commands::{Api, ApiConnectionArgs};
use crate::cli_contract::{CliError, print_json};

/// The OpenAPI document this CLI was built with.
const OPENAPI: &str = include_str!("../../../apps/docs/public/openapi.json");

const METHODS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

#[derive(Debug, Args)]
#[command(after_help = "Examples:
  oore api GET '/v1/builds?limit=5'
  oore api list_builds -q limit=5 -q status=failed
  oore api get_build -p build_id=b_123
  oore api POST /v1/builds/b_123/cancel
  oore api update_version_release_notes -p project_id=p1 -p version_name=1.2.0 -f text='Fixes'
  oore api list_projects --paginate
  oore api --list --tag Builds")]
pub(crate) struct ApiArgs {
    /// An HTTP method followed by a path, or an operation ID such as
    /// list_builds.
    #[arg(value_name = "METHOD|OPERATION")]
    target: Option<String>,

    /// The path, such as /v1/builds, when the first argument is a method.
    #[arg(value_name = "PATH")]
    path: Option<String>,

    /// A path parameter, as key=value, for an operation ID's path.
    #[arg(short = 'p', long = "path-param", value_name = "KEY=VALUE")]
    path_params: Vec<String>,

    /// A query parameter, as key=value. Repeat for several.
    #[arg(short = 'q', long = "query", value_name = "KEY=VALUE")]
    query: Vec<String>,

    /// A string field of the JSON body, as key=value. On GET it becomes a
    /// query parameter.
    #[arg(short = 'f', long = "field", value_name = "KEY=VALUE")]
    fields: Vec<String>,

    /// A typed field of the JSON body, as key=value, where the value is
    /// JSON: true, 3, null, ["a"] or {"a": 1}.
    #[arg(short = 'F', long = "typed-field", value_name = "KEY=VALUE")]
    typed_fields: Vec<String>,

    /// Read the request body from a file, or `-` for standard input.
    #[arg(long, value_name = "FILE", conflicts_with_all = ["fields", "typed_fields"])]
    input: Option<String>,

    /// Follow limit and offset lists until `total` is reached, and print one
    /// merged answer.
    #[arg(long)]
    paginate: bool,

    /// Write the answer to this file instead of printing it, for downloads.
    #[arg(short = 'o', long, value_name = "FILE")]
    output: Option<std::path::PathBuf>,

    /// List the operations instead of calling one.
    #[arg(long)]
    list: bool,

    /// With --list, only operations with this tag, such as Builds.
    #[arg(long, requires = "list")]
    tag: Option<String>,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Answers are JSON already; with --json a failure is a JSON object on
    /// stderr too.
    #[arg(long)]
    json: bool,
}

/// One operation of the OpenAPI document.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Operation {
    pub(crate) id: String,
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) summary: Option<String>,
    pub(crate) tags: Vec<String>,
}

pub(crate) fn operations() -> anyhow::Result<Vec<Operation>> {
    let spec: serde_json::Value =
        serde_json::from_str(OPENAPI).context("the embedded OpenAPI document is not valid")?;
    let mut operations = Vec::new();
    let paths = spec["paths"].as_object().cloned().unwrap_or_default();
    for (path, item) in paths {
        let Some(item) = item.as_object() else {
            continue;
        };
        for (method, operation) in item {
            let method = method.to_ascii_uppercase();
            if !METHODS.contains(&method.as_str()) {
                continue;
            }
            let Some(id) = operation["operationId"].as_str() else {
                continue;
            };
            operations.push(Operation {
                id: id.to_string(),
                method,
                path: path.clone(),
                summary: operation["summary"].as_str().map(str::to_string),
                tags: operation["tags"]
                    .as_array()
                    .map(|tags| {
                        tags.iter()
                            .filter_map(|tag| tag.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
            });
        }
    }
    operations.sort_by(|a, b| a.path.cmp(&b.path).then(a.method.cmp(&b.method)));
    Ok(operations)
}

fn key_value(raw: &str, flag: &str) -> anyhow::Result<(String, String)> {
    let (key, value) = raw
        .split_once('=')
        .ok_or_else(|| CliError::usage(format!("{flag} '{raw}' is not in the form key=value")))?;
    let key = key.trim();
    if key.is_empty() {
        return Err(CliError::usage(format!("{flag} '{raw}' has no key")).into());
    }
    Ok((key.to_string(), value.to_string()))
}

/// Puts the path parameters into an operation's path template.
fn fill_path(template: &str, params: &BTreeMap<String, String>) -> anyhow::Result<String> {
    let mut out = String::new();
    let mut missing = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let end = rest[start..]
            .find('}')
            .map(|end| start + end)
            .context("the operation's path is not valid")?;
        let name = &rest[start + 1..end];
        match params.get(name) {
            Some(value) => out.push_str(
                &url::form_urlencoded::byte_serialize(value.as_bytes())
                    .collect::<String>()
                    .replace('+', "%20"),
            ),
            None => missing.push(name.to_string()),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    if !missing.is_empty() {
        let flags = missing
            .iter()
            .map(|name| format!("-p {name}=..."))
            .collect::<Vec<_>>()
            .join(" ");
        return Err(CliError::usage(format!("{template} needs {flags}")).into());
    }
    Ok(out)
}

/// The method and path a call goes to.
fn target(args: &ApiArgs) -> anyhow::Result<(reqwest::Method, String)> {
    let first = args.target.as_deref().map(str::trim).unwrap_or_default();
    if first.is_empty() {
        return Err(
            CliError::usage("name an operation ID, or a method and a path")
                .with_hint("Run `oore api --list` to see the operations.")
                .into(),
        );
    }
    let mut params = BTreeMap::new();
    for raw in &args.path_params {
        let (key, value) = key_value(raw, "-p")?;
        params.insert(key, value);
    }
    if METHODS.contains(&first.to_ascii_uppercase().as_str()) {
        let path = args
            .path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .ok_or_else(|| CliError::usage(format!("{first} needs a path, such as /v1/builds")))?;
        let path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        let path = if path.contains('{') {
            fill_path(&path, &params)?
        } else {
            path
        };
        let method = reqwest::Method::from_bytes(first.to_ascii_uppercase().as_bytes())?;
        return Ok((method, path));
    }
    if args.path.is_some() {
        return Err(CliError::usage(format!(
            "'{first}' is not an HTTP method; an operation ID takes no path"
        ))
        .into());
    }
    let operations = operations()?;
    let operation = operations
        .iter()
        .find(|operation| operation.id == first)
        .ok_or_else(|| {
            let close: Vec<&str> = operations
                .iter()
                .filter(|operation| operation.id.contains(first) || first.contains(&operation.id))
                .take(5)
                .map(|operation| operation.id.as_str())
                .collect();
            let mut error = CliError::usage(format!("no operation is called '{first}'"));
            error = if close.is_empty() {
                error.with_hint("Run `oore api --list` to see the operations.")
            } else {
                error.with_hint(format!("Did you mean {}?", close.join(", ")))
            };
            error
        })?;
    let method = reqwest::Method::from_bytes(operation.method.as_bytes())?;
    Ok((method, fill_path(&operation.path, &params)?))
}

fn body(args: &ApiArgs) -> anyhow::Result<Option<Vec<u8>>> {
    if let Some(input) = args.input.as_deref() {
        let bytes = if input == "-" {
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut std::io::stdin(), &mut bytes)
                .context("failed to read standard input")?;
            bytes
        } else {
            std::fs::read(input).with_context(|| format!("failed to read {input}"))?
        };
        return Ok(Some(bytes));
    }
    if args.fields.is_empty() && args.typed_fields.is_empty() {
        return Ok(None);
    }
    let mut object = serde_json::Map::new();
    for raw in &args.fields {
        let (key, value) = key_value(raw, "-f")?;
        object.insert(key, serde_json::Value::String(value));
    }
    for raw in &args.typed_fields {
        let (key, value) = key_value(raw, "-F")?;
        let value = serde_json::from_str(&value).unwrap_or(serde_json::Value::String(value));
        object.insert(key, value);
    }
    Ok(Some(serde_json::to_vec(&object)?))
}

/// The array a list answer holds, such as `builds` in `{"builds", "total"}`.
fn list_field(value: &serde_json::Value) -> Option<String> {
    let object = value.as_object()?;
    object.get("total")?.as_i64()?;
    let arrays: Vec<&String> = object
        .iter()
        .filter(|(_, value)| value.is_array())
        .map(|(key, _)| key)
        .collect();
    (arrays.len() == 1).then(|| arrays[0].clone())
}

fn list_operations(args: &ApiArgs) -> anyhow::Result<()> {
    let operations: Vec<Operation> = operations()?
        .into_iter()
        .filter(|operation| {
            args.tag.as_deref().is_none_or(|tag| {
                operation
                    .tags
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(tag.trim()))
            })
        })
        .collect();
    if args.json {
        return print_json(&operations);
    }
    let rows: Vec<Vec<String>> = operations
        .iter()
        .map(|operation| {
            vec![
                operation.id.clone(),
                operation.method.clone(),
                operation.path.clone(),
                operation.summary.clone().unwrap_or_default(),
            ]
        })
        .collect();
    print!(
        "{}",
        crate::project_commands::render_table(&["OPERATION", "METHOD", "PATH", "SUMMARY"], &rows)
    );
    Ok(())
}

async fn call(
    api: &Api,
    method: &reqwest::Method,
    path: &str,
    query: &[(String, String)],
    body: Option<&[u8]>,
) -> anyhow::Result<reqwest::Response> {
    let mut request = api.client.request(method.clone(), api.url(path));
    if !query.is_empty() {
        request = request.query(query);
    }
    if let Some(body) = body {
        request = request
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_vec());
    }
    let response = api.execute(request).await?;
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    Err(CliError::from_api(status, &format!("{method} {path}"), &text).into())
}

pub(crate) async fn handle_api(args: ApiArgs) -> anyhow::Result<()> {
    if args.list {
        return list_operations(&args);
    }
    let (method, path) = target(&args)?;
    let mut query = Vec::new();
    for raw in &args.query {
        query.push(key_value(raw, "-q")?);
    }
    let mut body = body(&args)?;
    if method == reqwest::Method::GET && args.input.is_none() && body.is_some() {
        for raw in &args.fields {
            query.push(key_value(raw, "-f")?);
        }
        for raw in &args.typed_fields {
            query.push(key_value(raw, "-F")?);
        }
        body = None;
    }
    let api = Api::connect(&args.connection)?;

    if args.paginate {
        let page_size = query
            .iter()
            .find(|(key, _)| key == "limit")
            .and_then(|(_, value)| value.parse::<usize>().ok())
            .unwrap_or(100);
        query.retain(|(key, _)| key != "limit" && key != "offset");
        let mut merged: Option<serde_json::Value> = None;
        let mut field: Option<String> = None;
        let mut offset = 0usize;
        loop {
            let mut page_query = query.clone();
            page_query.push(("limit".into(), page_size.to_string()));
            page_query.push(("offset".into(), offset.to_string()));
            let response = call(&api, &method, &path, &page_query, body.as_deref()).await?;
            let page: serde_json::Value = response
                .json()
                .await
                .context("--paginate needs a JSON list answer")?;
            let name = field.clone().or_else(|| list_field(&page)).ok_or_else(|| {
                CliError::usage("--paginate needs an answer with `total` and one list")
            })?;
            let items = page[&name].as_array().cloned().unwrap_or_default();
            let total = page["total"].as_i64().unwrap_or(0);
            offset += items.len();
            match merged.as_mut() {
                None => merged = Some(page),
                Some(merged) => {
                    if let Some(list) = merged[&name].as_array_mut() {
                        list.extend(items.iter().cloned());
                    }
                }
            }
            field = Some(name);
            if items.is_empty() || i64::try_from(offset).unwrap_or(i64::MAX) >= total {
                break;
            }
        }
        return print_json(&merged.unwrap_or(serde_json::Value::Null));
    }

    let response = call(&api, &method, &path, &query, body.as_deref()).await?;
    let is_json = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("json"));
    let bytes = response
        .bytes()
        .await
        .context("failed to read the answer")?;
    if let Some(output) = &args.output {
        std::fs::write(output, &bytes)
            .with_context(|| format!("failed to write {}", output.display()))?;
        eprintln!("Saved {} ({} bytes)", output.display(), bytes.len());
        return Ok(());
    }
    if bytes.is_empty() {
        return Ok(());
    }
    if is_json && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
        return print_json(&value);
    }
    use std::io::Write;
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&bytes)?;
    if !bytes.ends_with(b"\n") {
        writeln!(stdout)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(argv: &[&str]) -> ApiArgs {
        #[derive(clap::Parser)]
        struct Wrapper {
            #[command(flatten)]
            args: ApiArgs,
        }
        use clap::Parser;
        Wrapper::parse_from(std::iter::once("oore").chain(argv.iter().copied())).args
    }

    #[test]
    fn every_operation_in_the_spec_is_listed_once() {
        let operations = operations().unwrap();
        assert!(operations.len() > 200);
        let mut ids: Vec<&str> = operations.iter().map(|op| op.id.as_str()).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "operation IDs are unique");
        let list = operations.iter().find(|op| op.id == "list_builds").unwrap();
        assert_eq!(
            (list.method.as_str(), list.path.as_str()),
            ("GET", "/v1/builds")
        );
    }

    #[test]
    fn an_operation_id_fills_its_path() {
        let (method, path) = target(&args(&["get_build", "-p", "build_id=b 1/x"])).unwrap();
        assert_eq!(method, reqwest::Method::GET);
        assert_eq!(path, "/v1/builds/b%201%2Fx");

        let error = target(&args(&["get_build"])).unwrap_err();
        let failure = crate::cli_contract::describe_failure(&error);
        assert_eq!(failure.exit, crate::cli_contract::exit::USAGE);
        assert!(failure.message.contains("-p build_id=..."));

        let error = target(&args(&["build"])).unwrap_err();
        assert!(
            crate::cli_contract::describe_failure(&error)
                .hint
                .unwrap()
                .contains("Did you mean")
        );
    }

    #[test]
    fn a_method_takes_a_path() {
        let (method, path) = target(&args(&["post", "v1/builds/b1/cancel"])).unwrap();
        assert_eq!(method, reqwest::Method::POST);
        assert_eq!(path, "/v1/builds/b1/cancel");
        assert!(target(&args(&["GET"])).is_err());
        assert!(target(&args(&["list_builds", "/v1/builds"])).is_err());
    }

    #[test]
    fn fields_make_a_json_body() {
        let body = body(&args(&[
            "POST",
            "/x",
            "-f",
            "text=1.2 fixes",
            "-F",
            "count=3",
            "-F",
            "tags=[\"a\"]",
            "-F",
            "word=plain",
        ]))
        .unwrap()
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"text": "1.2 fixes", "count": 3, "tags": ["a"], "word": "plain"})
        );
        assert!(super::body(&args(&["GET", "/x"])).unwrap().is_none());
    }

    #[test]
    fn list_answers_are_found_by_total() {
        assert_eq!(
            list_field(&serde_json::json!({"builds": [], "total": 3})),
            Some("builds".to_string())
        );
        assert_eq!(list_field(&serde_json::json!({"builds": []})), None);
    }
}
