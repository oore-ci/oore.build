//! The MCP tools. Each tool is a thin call of one or a few API operations
//! through [`ApiClient`], so permissions, token limits and audit entries
//! are the API's own.

use std::time::Instant;

use rmcp::model::{CallToolResult, ContentBlock, JsonObject, Tool, ToolAnnotations};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::client::{ApiClient, ApiFailure, MAX_WAIT};
use super::logs;
use super::shape;

/// What a tool may change, which decides how it is marked and who sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Reads only.
    Read,
    /// Starts, stops or changes builds, notes and tester shares.
    Build,
}

pub struct ToolSpec {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub kind: Kind,
    /// Stops or throws away work, such as cancelling a build.
    pub destructive: bool,
    /// Calling it again with the same arguments changes nothing more.
    pub idempotent: bool,
    pub schema: fn() -> Value,
}

const APP: &str = "App ID or exact app name.";
const PIPELINE: &str = "Pipeline ID, or its exact name within the app.";
const BUILD_ID: &str = "Build ID, as list_builds or run_build returns it.";
const IDEMPOTENCY: &str =
    "Optional. Send the same key when retrying the same request, so it runs only once.";

fn object(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

pub const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "list_apps",
        title: "List apps",
        description: "Lists the apps (Oore projects) you can see, with their IDs, platforms and repository.",
        kind: Kind::Read,
        destructive: false,
        idempotent: true,
        schema: || {
            object(
                json!({
                    "search": {"type": "string", "description": "Only apps whose name contains this."},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100, "default": 25},
                }),
                &[],
            )
        },
    },
    ToolSpec {
        name: "list_pipelines",
        title: "List pipelines",
        description: "Lists an app's pipelines: what each builds and whether its .oore.yaml is usable.",
        kind: Kind::Read,
        destructive: false,
        idempotent: true,
        schema: || {
            object(
                json!({
                    "app": {"type": "string", "description": APP},
                    "search": {"type": "string", "description": "Only pipelines whose name contains this."},
                }),
                &["app"],
            )
        },
    },
    ToolSpec {
        name: "list_builds",
        title: "List builds",
        description: "Lists recent builds, newest first. Filter by app, pipeline, status, branch or commit.",
        kind: Kind::Read,
        destructive: false,
        idempotent: true,
        schema: || {
            object(
                json!({
                    "app": {"type": "string", "description": APP},
                    "pipeline": {"type": "string", "description": PIPELINE},
                    "status": {"type": "string", "enum": ["queued", "scheduled", "assigned", "running", "succeeded", "failed", "canceled", "timed_out", "expired"]},
                    "branch": {"type": "string"},
                    "commit_sha": {"type": "string", "description": "Full commit SHA or a prefix of it."},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 50, "default": 10},
                }),
                &[],
            )
        },
    },
    ToolSpec {
        name: "get_build",
        title: "Get a build",
        description: "One build: status, steps with their results, commit, and store release outcome.",
        kind: Kind::Read,
        destructive: false,
        idempotent: true,
        schema: || {
            object(
                json!({"build_id": {"type": "string", "description": BUILD_ID}}),
                &["build_id"],
            )
        },
    },
    ToolSpec {
        name: "get_build_logs",
        title: "Read build logs",
        description: "Reads a build's log. Returns the last lines by default; narrow with step and search. \
                      Each line starts with its sequence number. Says when lines were left out.",
        kind: Kind::Read,
        destructive: false,
        idempotent: true,
        schema: || {
            object(
                json!({
                    "build_id": {"type": "string", "description": BUILD_ID},
                    "step": {"type": "string", "description": "Only this step's lines, by its name as get_build lists it."},
                    "search": {"type": "string", "description": "Only lines that contain this text, ignoring case."},
                    "tail": {"type": "integer", "minimum": 1, "maximum": logs::MAX_TAIL, "default": logs::DEFAULT_TAIL, "description": "How many of the last matching lines to return."},
                    "after_sequence": {"type": "integer", "description": "Only lines after this sequence, to read on from an earlier call."},
                }),
                &["build_id"],
            )
        },
    },
    ToolSpec {
        name: "diagnose_build",
        title: "Explain a failed build",
        description: "Why a build failed: the failed step and exit code, the likely cause and fix, \
                      the last lines of that step, and failed tests. Start here for any failed build.",
        kind: Kind::Read,
        destructive: false,
        idempotent: true,
        schema: || {
            object(
                json!({
                    "build_id": {"type": "string", "description": BUILD_ID},
                    "tail": {"type": "integer", "minimum": 1, "maximum": 200, "default": 40, "description": "How many log lines of the failed step to include."},
                }),
                &["build_id"],
            )
        },
    },
    ToolSpec {
        name: "list_artifacts",
        title: "List build artifacts",
        description: "The files a build produced (APK, AAB, IPA and others), with their IDs and sizes.",
        kind: Kind::Read,
        destructive: false,
        idempotent: true,
        schema: || {
            object(
                json!({"build_id": {"type": "string", "description": BUILD_ID}}),
                &["build_id"],
            )
        },
    },
    ToolSpec {
        name: "get_install_link",
        title: "Get an install link",
        description: "A short-lived link that installs an artifact on a phone, and a direct download link.",
        kind: Kind::Read,
        destructive: false,
        idempotent: false,
        schema: || {
            object(
                json!({"artifact_id": {"type": "string", "description": "Artifact ID, from list_artifacts."}}),
                &["artifact_id"],
            )
        },
    },
    ToolSpec {
        name: "list_releases",
        title: "List store releases",
        description: "Store releases on Google Play and the App Store: state, track, rollout and what can be done next.",
        kind: Kind::Read,
        destructive: false,
        idempotent: true,
        schema: || {
            object(
                json!({
                    "app": {"type": "string", "description": APP},
                    "build_id": {"type": "string"},
                    "state": {"type": "string", "description": "Only releases in this state, such as rolling_out or in_review."},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 50, "default": 10},
                }),
                &[],
            )
        },
    },
    ToolSpec {
        name: "get_release",
        title: "Get a store release",
        description: "One store release, with its state, the store's last answer, and its allowed actions.",
        kind: Kind::Read,
        destructive: false,
        idempotent: true,
        schema: || object(json!({"release_id": {"type": "string"}}), &["release_id"]),
    },
    ToolSpec {
        name: "get_store_readiness",
        title: "Check store readiness",
        description: "Whether a pipeline is ready to publish to the stores, and what blocks it.",
        kind: Kind::Read,
        destructive: false,
        idempotent: true,
        schema: || {
            object(
                json!({
                    "pipeline": {"type": "string", "description": PIPELINE},
                    "app": {"type": "string", "description": "App ID or name. Needed when pipeline is a name."},
                }),
                &["pipeline"],
            )
        },
    },
    ToolSpec {
        name: "validate_pipeline_file",
        title: "Check a pipeline file",
        description: "Checks the text of an .oore.yaml file and lists its errors, without saving anything.",
        kind: Kind::Read,
        destructive: false,
        idempotent: true,
        schema: || {
            object(
                json!({
                    "yaml": {"type": "string", "description": "The whole .oore.yaml file."},
                    "workflow": {"type": "string", "description": "Check this workflow of the file."},
                }),
                &["yaml"],
            )
        },
    },
    ToolSpec {
        name: "list_workflows",
        title: "List workflows",
        description: "The workflows a build of this pipeline can run, read from its .oore.yaml at a branch, tag or commit.",
        kind: Kind::Read,
        destructive: false,
        idempotent: true,
        schema: || {
            object(
                json!({
                    "pipeline": {"type": "string", "description": PIPELINE},
                    "app": {"type": "string", "description": "App ID or name. Needed when pipeline is a name."},
                    "ref": {"type": "string", "description": "Branch, tag or commit. Default: the default branch."},
                }),
                &["pipeline"],
            )
        },
    },
    ToolSpec {
        name: "list_tester_groups",
        title: "List tester groups",
        description: "The tester groups a build can be sent to, with their IDs.",
        kind: Kind::Read,
        destructive: false,
        idempotent: true,
        schema: || object(json!({}), &[]),
    },
    ToolSpec {
        name: "run_build",
        title: "Start a build",
        description: "Starts a build of a pipeline. Returns the new build at once; then call wait_for_build.",
        kind: Kind::Build,
        destructive: false,
        idempotent: false,
        schema: || {
            object(
                json!({
                    "app": {"type": "string", "description": APP},
                    "pipeline": {"type": "string", "description": PIPELINE},
                    "branch": {"type": "string", "description": "Default: the app's default branch."},
                    "commit_sha": {"type": "string"},
                    "workflow": {"type": "string", "description": "Workflow from the .oore.yaml. Default: the pipeline's."},
                    "platforms": {"type": "array", "items": {"type": "string", "enum": ["android", "ios"]}, "description": "Build only these platforms."},
                    "changelog": {"type": "string", "description": "What changed, for testers."},
                    "store_notes": {"type": "string", "description": "What's new text for the stores."},
                    "idempotency_key": {"type": "string", "description": IDEMPOTENCY},
                }),
                &["app", "pipeline"],
            )
        },
    },
    ToolSpec {
        name: "wait_for_build",
        title: "Wait for a build",
        description: "Waits up to about 50 seconds for a build to finish, then answers with its status. \
                      If done is false, call it again after next_check_seconds. Builds take 10 to 40 minutes.",
        kind: Kind::Read,
        destructive: false,
        idempotent: true,
        schema: || {
            object(
                json!({
                    "build_id": {"type": "string", "description": BUILD_ID},
                    "timeout_seconds": {"type": "integer", "minimum": 1, "maximum": 50, "default": 45},
                }),
                &["build_id"],
            )
        },
    },
    ToolSpec {
        name: "cancel_build",
        title: "Cancel a build",
        description: "Stops a queued or running build.",
        kind: Kind::Build,
        destructive: true,
        idempotent: true,
        schema: || {
            object(
                json!({"build_id": {"type": "string", "description": BUILD_ID}}),
                &["build_id"],
            )
        },
    },
    ToolSpec {
        name: "rerun_build",
        title: "Re-run a build",
        description: "Starts a new build with the same commit and settings as an earlier one.",
        kind: Kind::Build,
        destructive: false,
        idempotent: false,
        schema: || {
            object(
                json!({
                    "build_id": {"type": "string", "description": BUILD_ID},
                    "idempotency_key": {"type": "string", "description": IDEMPOTENCY},
                }),
                &["build_id"],
            )
        },
    },
    ToolSpec {
        name: "retry_step",
        title: "Retry from a step",
        description: "Starts a new build that reuses an earlier build's results up to a step and runs from that step on, \
                      such as retrying only a failed store upload.",
        kind: Kind::Build,
        destructive: false,
        idempotent: false,
        schema: || {
            object(
                json!({
                    "build_id": {"type": "string", "description": BUILD_ID},
                    "step": {"type": "string", "description": "Step name, from the build's retry_steps."},
                    "idempotency_key": {"type": "string", "description": IDEMPOTENCY},
                }),
                &["build_id", "step"],
            )
        },
    },
    ToolSpec {
        name: "set_release_notes",
        title: "Set release notes",
        description: "Saves the release notes of an app version (for testers and the stores). Replaces the saved text.",
        kind: Kind::Build,
        destructive: false,
        idempotent: true,
        schema: || {
            object(
                json!({
                    "app": {"type": "string", "description": APP},
                    "version_name": {"type": "string", "description": "App version, such as 2.4.0."},
                    "text": {"type": "string", "maxLength": 4000, "description": "Notes in the default language."},
                    "locales": {
                        "type": "array",
                        "description": "Other languages. Replaces the saved ones.",
                        "items": {
                            "type": "object",
                            "properties": {"locale": {"type": "string"}, "text": {"type": "string"}},
                            "required": ["locale", "text"],
                        },
                    },
                }),
                &["app", "version_name", "text"],
            )
        },
    },
    ToolSpec {
        name: "share_build_with_testers",
        title: "Send a build to testers",
        description: "Sends a build to a tester group, who get an install link. Find group IDs with list_tester_groups.",
        kind: Kind::Build,
        destructive: false,
        idempotent: false,
        schema: || {
            object(
                json!({
                    "build_id": {"type": "string", "description": BUILD_ID},
                    "group_id": {"type": "string"},
                    "message": {"type": "string", "description": "A note for the testers."},
                    "idempotency_key": {"type": "string", "description": IDEMPOTENCY},
                }),
                &["build_id", "group_id"],
            )
        },
    },
];

pub fn find(name: &str) -> Option<&'static ToolSpec> {
    TOOLS.iter().find(|tool| tool.name == name)
}

/// The MCP description of a tool.
pub fn describe(spec: &ToolSpec) -> Tool {
    let schema: JsonObject = match (spec.schema)() {
        Value::Object(map) => map,
        _ => JsonObject::new(),
    };
    let annotations = ToolAnnotations::with_title(spec.title)
        .read_only(spec.kind == Kind::Read)
        .destructive(spec.destructive)
        .idempotent(spec.idempotent)
        .open_world(false);
    let output: JsonObject = match output_schema(spec.name) {
        Value::Object(map) => map,
        _ => JsonObject::new(),
    };
    Tool::new(spec.name, spec.description, std::sync::Arc::new(schema))
        .with_title(spec.title)
        .with_annotations(annotations)
        .with_raw_output_schema(std::sync::Arc::new(output))
}

/// What a tool answers. Loose on purpose: the main fields are named and
/// typed, and the API may add more.
fn output_schema(name: &str) -> Value {
    let array = |description: &str| json!({"type": "array", "description": description, "items": {"type": "object"}});
    let build = json!({"type": "object", "description": "A build: id, build_number, status, branch, commit, steps."});
    let (properties, required): (Value, &[&str]) = match name {
        "list_apps" => (
            json!({"apps": array("Apps: id, name, platforms, repository."), "total": {"type": "integer"}}),
            &["apps"],
        ),
        "list_pipelines" => (
            json!({"app_id": {"type": "string"}, "pipelines": array("Pipelines: id, name, workflow, config_path, health.")}),
            &["pipelines"],
        ),
        "list_builds" => (
            json!({"builds": array("Builds, newest first."), "total": {"type": "integer"}}),
            &["builds"],
        ),
        "get_build" => (
            json!({"id": {"type": "string"}, "status": {"type": "string"}, "steps": array("Steps with status and exit code.")}),
            &["id", "status"],
        ),
        "get_build_logs" => (
            json!({
                "lines": {"type": "array", "items": {"type": "string"}, "description": "Log lines, each starting with its sequence."},
                "returned": {"type": "integer"},
                "matching_lines": {"type": "integer"},
                "truncated": {"type": "boolean", "description": "True when matching lines were left out."},
                "steps": {"type": "array", "items": {"type": "string"}},
                "note": {"type": "string"},
            }),
            &["lines", "truncated"],
        ),
        "diagnose_build" => (
            json!({
                "build_id": {"type": "string"},
                "status": {"type": "string"},
                "failed_step": {"type": ["object", "null"]},
                "hint": {"type": ["object", "null"], "description": "Likely cause and fix."},
                "log_tail": {"type": "array", "items": {"type": "string"}},
                "log_tail_truncated": {"type": "boolean"},
                "failed_tests": array("Failed tests: name, message."),
                "failed_tests_total": {"type": "integer"},
            }),
            &["status"],
        ),
        "list_artifacts" => (
            json!({"artifacts": array("Artifacts: id, name, artifact_type, file_size.")}),
            &["artifacts"],
        ),
        "get_install_link" => (
            json!({"install_url": {"type": "string"}, "download_url": {"type": "string"}, "expires_at": {"type": "integer"}}),
            &["install_url"],
        ),
        "list_releases" => (
            json!({"releases": array("Store releases."), "total": {"type": "integer"}}),
            &["releases"],
        ),
        "get_release" => (
            json!({"id": {"type": "string"}, "state": {"type": "string"}, "allowed_actions": {"type": "array", "items": {"type": "string"}}}),
            &["id", "state"],
        ),
        "get_store_readiness" => (
            json!({"pipeline_id": {"type": "string"}, "items": array("Readiness checks.")}),
            &[],
        ),
        "validate_pipeline_file" => (
            json!({"valid": {"type": "boolean"}, "errors": {"type": "array", "items": {"type": "string"}}}),
            &["valid", "errors"],
        ),
        "list_workflows" => (
            json!({"workflows": array("Workflows in the file."), "errors": {"type": "array", "items": {"type": "string"}}}),
            &[],
        ),
        "list_tester_groups" => (
            json!({"groups": array("Groups: id, name, member_count.")}),
            &["groups"],
        ),
        "run_build" | "rerun_build" | "retry_step" => (
            json!({"build": build, "next": {"type": "string"}}),
            &["build"],
        ),
        "wait_for_build" => (
            json!({
                "done": {"type": "boolean"},
                "status": {"type": "string"},
                "build": build,
                "next_check_seconds": {"type": "integer", "description": "When done is false: wait this long, then call again."},
                "next": {"type": "string"},
            }),
            &["done", "status"],
        ),
        "cancel_build" => (json!({"build": build}), &[]),
        "share_build_with_testers" => (
            json!({"share_id": {"type": "string"}, "group_name": {"type": "string"}, "recipients": {"type": "integer"}}),
            &[],
        ),
        _ => (json!({}), &[]),
    };
    json!({"type": "object", "properties": properties, "required": required})
}

/// A tool failure, shown to the agent as the tool's result.
#[derive(Debug)]
pub struct ToolError {
    pub message: String,
    pub code: String,
    pub status: Option<u16>,
    pub hint: Option<String>,
}

impl ToolError {
    pub fn usage(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: "invalid_arguments".into(),
            status: None,
            hint: None,
        }
    }

    pub fn into_result(self) -> CallToolResult {
        let mut body = json!({"error": self.message, "code": self.code});
        if let Some(status) = self.status {
            body["status"] = json!(status);
        }
        if let Some(hint) = self.hint {
            body["hint"] = json!(hint);
        }
        // No structured content: it would have to match the success schema.
        CallToolResult::error(vec![ContentBlock::text(body.to_string())])
    }
}

impl From<ApiFailure> for ToolError {
    fn from(failure: ApiFailure) -> Self {
        let hint = failure.hint.or_else(|| match failure.status.as_u16() {
            401 => Some(
                "The token is missing, expired or revoked. Create a new API token in Oore.".into(),
            ),
            403 => Some("This token or its owner may not do this.".into()),
            _ => None,
        });
        Self {
            message: failure.message,
            code: failure.code,
            status: Some(failure.status.as_u16()),
            hint,
        }
    }
}

type ToolResult = Result<Value, ToolError>;

/// Arguments of one call.
struct Args<'a>(&'a JsonObject);

impl Args<'_> {
    fn opt_str(&self, name: &str) -> Result<Option<String>, ToolError> {
        match self.0.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(value)) => {
                let value = value.trim();
                Ok((!value.is_empty()).then(|| value.to_string()))
            }
            Some(_) => Err(ToolError::usage(format!("{name} must be a string"))),
        }
    }

    fn str(&self, name: &str) -> Result<String, ToolError> {
        self.opt_str(name)?
            .ok_or_else(|| ToolError::usage(format!("{name} is required")))
    }

    fn opt_int(&self, name: &str) -> Result<Option<i64>, ToolError> {
        match self.0.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => value
                .as_i64()
                .or_else(|| value.as_str().and_then(|s| s.trim().parse().ok()))
                .map(Some)
                .ok_or_else(|| ToolError::usage(format!("{name} must be a whole number"))),
        }
    }

    fn limit(&self, name: &str, default: i64, max: i64) -> Result<i64, ToolError> {
        Ok(self.opt_int(name)?.unwrap_or(default).clamp(1, max))
    }

    fn raw(&self, name: &str) -> Option<&Value> {
        self.0.get(name).filter(|value| !value.is_null())
    }
}

fn enc(value: &str) -> String {
    urlencoding::encode(value).into_owned()
}

/// Builds a query string from the values that are set.
fn query(pairs: &[(&str, Option<String>)]) -> String {
    let parts: Vec<String> = pairs
        .iter()
        .filter_map(|(key, value)| value.as_ref().map(|value| format!("{key}={}", enc(value))))
        .collect();
    if parts.is_empty() {
        String::new()
    } else {
        format!("?{}", parts.join("&"))
    }
}

/// Finds an app by ID, or by its exact name.
async fn resolve_app(api: &ApiClient, value: &str) -> Result<Value, ToolError> {
    match api.get(&format!("/v1/projects/{}", enc(value))).await {
        Ok(detail) => return Ok(inner(detail, "project")),
        Err(failure) if failure.status.as_u16() == 404 => {}
        Err(failure) => return Err(failure.into()),
    }
    let found = api
        .get(&format!("/v1/projects?search={}&limit=100", enc(value)))
        .await?;
    let projects = found
        .get("projects")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let matches: Vec<Value> = projects
        .into_iter()
        .filter(|project| {
            project
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name.eq_ignore_ascii_case(value))
        })
        .collect();
    match matches.len() {
        1 => Ok(matches.into_iter().next().unwrap_or_default()),
        0 => Err(ToolError {
            message: format!("No app with the ID or name {value:?}."),
            code: "not_found".into(),
            status: Some(404),
            hint: Some("Call list_apps to see the apps you can reach.".into()),
        }),
        _ => Err(ToolError {
            message: format!("More than one app is named {value:?}."),
            code: "ambiguous".into(),
            status: None,
            hint: Some("Pass the app ID from list_apps.".into()),
        }),
    }
}

async fn resolve_app_id(api: &ApiClient, value: &str) -> Result<String, ToolError> {
    let app = resolve_app(api, value).await?;
    app.get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ToolError::usage("The app has no ID"))
}

/// Finds a pipeline by ID, or by its exact name within an app.
async fn resolve_pipeline(
    api: &ApiClient,
    value: &str,
    app: Option<&str>,
) -> Result<Value, ToolError> {
    match api.get(&format!("/v1/pipelines/{}", enc(value))).await {
        Ok(detail) => {
            let pipeline = inner(detail, "pipeline");
            if let Some(app) = app {
                let app_id = resolve_app_id(api, app).await?;
                if pipeline.get("project_id").and_then(Value::as_str) != Some(app_id.as_str()) {
                    return Err(ToolError {
                        message: format!("Pipeline {value:?} belongs to another app."),
                        code: "not_found".into(),
                        status: Some(404),
                        hint: Some("Call list_pipelines for the app.".into()),
                    });
                }
            }
            return Ok(pipeline);
        }
        Err(failure) if failure.status.as_u16() == 404 => {}
        Err(failure) => return Err(failure.into()),
    }
    let Some(app) = app else {
        return Err(ToolError {
            message: format!("No pipeline with the ID {value:?}."),
            code: "not_found".into(),
            status: Some(404),
            hint: Some("To find a pipeline by name, also pass app.".into()),
        });
    };
    let app_id = resolve_app_id(api, app).await?;
    let found = api
        .get(&format!(
            "/v1/projects/{}/pipelines?search={}&limit=100",
            enc(&app_id),
            enc(value)
        ))
        .await?;
    let matches: Vec<Value> = found
        .get("pipelines")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|pipeline| {
            pipeline
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name.eq_ignore_ascii_case(value))
        })
        .collect();
    match matches.len() {
        1 => Ok(matches.into_iter().next().unwrap_or_default()),
        0 => Err(ToolError {
            message: format!("The app has no pipeline with the ID or name {value:?}."),
            code: "not_found".into(),
            status: Some(404),
            hint: Some("Call list_pipelines to see the app's pipelines.".into()),
        }),
        _ => Err(ToolError {
            message: format!("More than one pipeline is named {value:?}."),
            code: "ambiguous".into(),
            status: None,
            hint: Some("Pass the pipeline ID from list_pipelines.".into()),
        }),
    }
}

/// Single-item answers wrap the item, such as `{"build": {...}}`.
fn inner(mut detail: Value, key: &str) -> Value {
    match detail.get_mut(key) {
        Some(item) if item.is_object() => item.take(),
        _ => detail,
    }
}

fn id_of(value: &Value) -> String {
    value
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn list(value: &Value, key: &str) -> Vec<Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// Runs one tool.
pub async fn call(
    spec: &ToolSpec,
    api: &ApiClient,
    arguments: &JsonObject,
    cancel: &CancellationToken,
) -> ToolResult {
    let args = Args(arguments);
    match spec.name {
        "list_apps" => {
            let limit = args.limit("limit", 25, 100)?;
            let found = api
                .get(&format!(
                    "/v1/projects{}",
                    query(&[
                        ("search", args.opt_str("search")?),
                        ("limit", Some(limit.to_string())),
                    ])
                ))
                .await?;
            let apps: Vec<Value> = list(&found, "projects").iter().map(shape::app).collect();
            Ok(
                json!({"apps": apps, "total": found.get("total").cloned().unwrap_or(json!(apps.len()))}),
            )
        }
        "list_pipelines" => {
            let app_id = resolve_app_id(api, &args.str("app")?).await?;
            let found = api
                .get(&format!(
                    "/v1/projects/{}/pipelines{}",
                    enc(&app_id),
                    query(&[
                        ("search", args.opt_str("search")?),
                        ("limit", Some("100".into())),
                    ])
                ))
                .await?;
            let pipelines: Vec<Value> = list(&found, "pipelines")
                .iter()
                .map(shape::pipeline)
                .collect();
            Ok(json!({"app_id": app_id, "pipelines": pipelines}))
        }
        "list_builds" => {
            let app = args.opt_str("app")?;
            let app_id = match app.as_deref() {
                Some(app) => Some(resolve_app_id(api, app).await?),
                None => None,
            };
            let pipeline_id = match args.opt_str("pipeline")? {
                Some(pipeline) => Some(id_of(
                    &resolve_pipeline(api, &pipeline, app.as_deref()).await?,
                )),
                None => None,
            };
            let limit = args.limit("limit", 10, 50)?;
            let found = api
                .get(&format!(
                    "/v1/builds{}",
                    query(&[
                        ("project_id", app_id),
                        ("pipeline_id", pipeline_id),
                        ("status", args.opt_str("status")?),
                        ("branch", args.opt_str("branch")?),
                        ("commit_sha", args.opt_str("commit_sha")?),
                        ("limit", Some(limit.to_string())),
                    ])
                ))
                .await?;
            let builds: Vec<Value> = list(&found, "builds")
                .iter()
                .map(shape::build_row)
                .collect();
            Ok(
                json!({"builds": builds, "total": found.get("total").cloned().unwrap_or(json!(builds.len()))}),
            )
        }
        "get_build" => {
            let detail = api
                .get(&format!("/v1/builds/{}", enc(&args.str("build_id")?)))
                .await?;
            let mut build = shape::build_detail(detail.get("build").unwrap_or(&Value::Null));
            for field in ["queue", "retryable_steps"] {
                if let Some(value) = detail.get(field).filter(|v| !v.is_null()) {
                    build[field] = value.clone();
                }
            }
            Ok(build)
        }
        "get_build_logs" => {
            let build_id = args.str("build_id")?;
            let lines = logs::read_all(api, &build_id).await?;
            let tail = args
                .opt_int("tail")?
                .map(|n| n.clamp(1, logs::MAX_TAIL as i64) as usize);
            let mut result = logs::select(
                &lines,
                &logs::Filter {
                    step: args.opt_str("step")?,
                    search: args.opt_str("search")?,
                    tail,
                    after_sequence: args.opt_int("after_sequence")?,
                },
            );
            result["build_id"] = json!(build_id);
            Ok(result)
        }
        "diagnose_build" => {
            diagnose(
                api,
                &args.str("build_id")?,
                args.limit("tail", 40, 200)? as usize,
            )
            .await
        }
        "list_artifacts" => {
            let found = api
                .get(&format!(
                    "/v1/builds/{}/artifacts",
                    enc(&args.str("build_id")?)
                ))
                .await?;
            let artifacts: Vec<Value> = list(&found, "artifacts")
                .iter()
                .map(shape::artifact)
                .collect();
            Ok(json!({"artifacts": artifacts}))
        }
        "get_install_link" => {
            let link = api
                .post(
                    &format!(
                        "/v1/artifacts/{}/install-link",
                        enc(&args.str("artifact_id")?)
                    ),
                    None,
                )
                .await?;
            Ok(link)
        }
        "list_releases" => {
            let app_id = match args.opt_str("app")? {
                Some(app) => Some(resolve_app_id(api, &app).await?),
                None => None,
            };
            let limit = args.limit("limit", 10, 50)?;
            let found = api
                .get(&format!(
                    "/v1/releases{}",
                    query(&[
                        ("project_id", app_id),
                        ("build_id", args.opt_str("build_id")?),
                        ("state", args.opt_str("state")?),
                        ("limit", Some(limit.to_string())),
                    ])
                ))
                .await?;
            let releases: Vec<Value> = list(&found, "releases")
                .iter()
                .map(shape::release)
                .collect();
            Ok(
                json!({"releases": releases, "total": found.get("total").cloned().unwrap_or(json!(releases.len()))}),
            )
        }
        "get_release" => {
            let release = api
                .get(&format!("/v1/releases/{}", enc(&args.str("release_id")?)))
                .await?;
            let mut result = shape::release(release.get("release").unwrap_or(&Value::Null));
            if let Some(history) = release.get("history").and_then(Value::as_array) {
                let recent: Vec<Value> = history.iter().rev().take(5).cloned().collect();
                result["recent_history"] = json!(recent);
            }
            Ok(result)
        }
        "get_store_readiness" => {
            let app = args.opt_str("app")?;
            let pipeline = resolve_pipeline(api, &args.str("pipeline")?, app.as_deref()).await?;
            Ok(api
                .get(&format!(
                    "/v1/pipelines/{}/publish/readiness",
                    enc(&id_of(&pipeline))
                ))
                .await?)
        }
        "validate_pipeline_file" => {
            let mut body = json!({"repository_yaml": args.str("yaml")?});
            if let Some(workflow) = args.opt_str("workflow")? {
                body["workflow"] = json!(workflow);
            }
            Ok(api.post("/v1/pipelines/validate", Some(body)).await?)
        }
        "list_workflows" => {
            let app = args.opt_str("app")?;
            let pipeline = resolve_pipeline(api, &args.str("pipeline")?, app.as_deref()).await?;
            Ok(api
                .get(&format!(
                    "/v1/pipelines/{}/workflows{}",
                    enc(&id_of(&pipeline)),
                    query(&[("ref", args.opt_str("ref")?)])
                ))
                .await?)
        }
        "list_tester_groups" => {
            let found = api.get("/v1/tester-groups").await?;
            let groups: Vec<Value> = list(&found, "groups")
                .iter()
                .map(shape::tester_group)
                .collect();
            Ok(json!({"groups": groups}))
        }
        "run_build" => {
            let app = args.str("app")?;
            let app_id = resolve_app_id(api, &app).await?;
            let pipeline = resolve_pipeline(api, &args.str("pipeline")?, Some(&app_id)).await?;
            let mut body = json!({"pipeline_id": id_of(&pipeline)});
            for field in [
                "branch",
                "commit_sha",
                "workflow",
                "changelog",
                "store_notes",
            ] {
                if let Some(value) = args.opt_str(field)? {
                    body[field] = json!(value);
                }
            }
            if let Some(platforms) = args.raw("platforms") {
                body["platforms"] = platforms.clone();
            }
            let created = api
                .post_idempotent(
                    &format!("/v1/projects/{}/builds", enc(&app_id)),
                    Some(body),
                    args.opt_str("idempotency_key")?.as_deref(),
                )
                .await?;
            Ok(started(&created))
        }
        "wait_for_build" => {
            let seconds = args.limit("timeout_seconds", 45, MAX_WAIT.as_secs() as i64)? as u64;
            wait_for_build(api, &args.str("build_id")?, seconds, cancel).await
        }
        "cancel_build" => {
            let answer = api
                .post(
                    &format!("/v1/builds/{}/cancel", enc(&args.str("build_id")?)),
                    None,
                )
                .await?;
            Ok(match answer.get("build") {
                Some(build) => json!({"build": shape::build_row(build)}),
                None => answer,
            })
        }
        "rerun_build" => {
            let created = api
                .post_idempotent(
                    &format!("/v1/builds/{}/rerun", enc(&args.str("build_id")?)),
                    None,
                    args.opt_str("idempotency_key")?.as_deref(),
                )
                .await?;
            Ok(started(&created))
        }
        "retry_step" => {
            let created = api
                .post_idempotent(
                    &format!("/v1/builds/{}/retry-step", enc(&args.str("build_id")?)),
                    Some(json!({"step": args.str("step")?})),
                    args.opt_str("idempotency_key")?.as_deref(),
                )
                .await?;
            Ok(started(&created))
        }
        "set_release_notes" => {
            let app_id = resolve_app_id(api, &args.str("app")?).await?;
            let mut body = json!({"text": args.str("text")?});
            if let Some(locales) = args.raw("locales") {
                body["locales"] = locales.clone();
            }
            Ok(api
                .put(
                    &format!(
                        "/v1/projects/{}/release-notes/{}",
                        enc(&app_id),
                        enc(&args.str("version_name")?)
                    ),
                    body,
                )
                .await?)
        }
        "share_build_with_testers" => {
            let mut body = json!({"group_id": args.str("group_id")?});
            if let Some(message) = args.opt_str("message")? {
                body["message"] = json!(message);
            }
            let share = api
                .post_idempotent(
                    &format!("/v1/builds/{}/shares", enc(&args.str("build_id")?)),
                    Some(body),
                    args.opt_str("idempotency_key")?.as_deref(),
                )
                .await?;
            let recipients = share
                .get("recipients")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            Ok(json!({
                "share_id": share.get("id"),
                "group_name": share.get("group_name"),
                "recipients": recipients,
                "sent_at": share.get("sent_at"),
            }))
        }
        other => Err(ToolError::usage(format!("Unknown tool {other}"))),
    }
}

fn started(created: &Value) -> Value {
    let build = created.get("build").unwrap_or(created);
    json!({
        "build": shape::build_row(build),
        "next": "Call wait_for_build with this build's id to follow it.",
    })
}

/// Waits for a build to finish, for at most `seconds`.
async fn wait_for_build(
    api: &ApiClient,
    build_id: &str,
    seconds: u64,
    cancel: &CancellationToken,
) -> ToolResult {
    let deadline = Instant::now() + std::time::Duration::from_secs(seconds);
    let path = format!("/v1/builds/{}", enc(build_id));
    let mut build = inner(api.get(&path).await?, "build");
    loop {
        let status = build
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if shape::is_terminal(status) {
            let mut result =
                json!({"done": true, "status": status, "build": shape::build_detail(&build)});
            if matches!(status, "failed" | "timed_out") {
                result["next"] = json!("Call diagnose_build to see why it failed.");
            }
            return Ok(result);
        }
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        let remaining = deadline - now;
        // A daemon that can hold the request answers when the build changes.
        // Others answer at once, and the pause below paces the polling.
        let updated = build.get("updated_at").and_then(Value::as_i64).unwrap_or(0);
        let wait = remaining.as_secs().clamp(1, 50);
        let asked = Instant::now();
        let poll = format!("{path}?wait={wait}&since_updated_at={updated}");
        let next: Value = tokio::select! {
            next = api.get(&poll) => next?,
            () = cancel.cancelled() => return Err(ToolError::usage("The wait was cancelled.")),
        };
        let next = inner(next, "build");
        let unchanged = next.get("updated_at") == build.get("updated_at");
        build = next;
        if unchanged && asked.elapsed() < std::time::Duration::from_secs(1) {
            let pause = remaining.min(std::time::Duration::from_secs(3));
            tokio::select! {
                () = tokio::time::sleep(pause) => {}
                () = cancel.cancelled() => return Err(ToolError::usage("The wait was cancelled.")),
            }
        }
    }
    let status = build
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let next_check = if matches!(status.as_str(), "queued" | "scheduled") {
        30
    } else {
        15
    };
    Ok(json!({
        "done": false,
        "status": status,
        "build": shape::build_row(&build),
        "next_check_seconds": next_check,
        "next": "The build is still going. Call wait_for_build again.",
    }))
}

/// Why a build failed. Uses the daemon's diagnosis operation where it has
/// one, and otherwise puts the same answer together from older operations.
async fn diagnose(api: &ApiClient, build_id: &str, tail: usize) -> ToolResult {
    let id = enc(build_id);
    match api
        .get(&format!("/v1/builds/{id}/diagnosis?tail={tail}"))
        .await
    {
        Ok(diagnosis) => return Ok(diagnosis),
        Err(failure) if failure.route_missing() => {}
        Err(failure) => return Err(failure.into()),
    }

    let build = inner(api.get(&format!("/v1/builds/{id}")).await?, "build");
    let status = build
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let failed_step = build
        .get("step_results")
        .and_then(Value::as_array)
        .and_then(|steps| {
            steps
                .iter()
                .find(|step| step.get("status").and_then(Value::as_str) == Some("failed"))
        })
        .cloned();
    let mut result = json!({
        "build_id": build.get("id"),
        "build_number": build.get("build_number"),
        "status": status,
        "failed_step": failed_step.as_ref().map(|step| json!({
            "name": step.get("name"),
            "exit_code": step.get("exit_code"),
        })),
    });
    if !matches!(status.as_str(), "failed" | "timed_out") {
        result["note"] = json!("The build has not failed.");
        return Ok(result);
    }

    let hint = api.get(&format!("/v1/builds/{id}/failure-hint")).await?;
    result["hint"] = hint.get("hint").cloned().unwrap_or(Value::Null);

    let lines = logs::read_all(api, build_id).await?;
    let step_name = failed_step
        .as_ref()
        .and_then(|step| step.get("name"))
        .and_then(Value::as_str);
    let step_has_lines =
        step_name.is_some_and(|name| lines.iter().any(|l| l.step.as_deref() == Some(name)));
    let (log_tail, cut) = logs::step_tail(&lines, step_name.filter(|_| step_has_lines), tail);
    result["log_tail"] = json!(log_tail);
    result["log_tail_truncated"] = json!(cut);

    match api.get(&format!("/v1/builds/{id}/tests")).await {
        Ok(tests) => {
            let failures = list(&tests, "failures");
            let shown: Vec<Value> = failures
                .iter()
                .take(10)
                .map(|test| {
                    let message: Option<String> = test
                        .get("message")
                        .and_then(Value::as_str)
                        .map(|m| m.chars().take(300).collect());
                    json!({"name": test.get("name"), "message": message})
                })
                .collect();
            result["failed_tests"] = json!(shown);
            result["failed_tests_total"] = json!(failures.len());
        }
        Err(failure) if failure.status.as_u16() == 404 => {
            result["failed_tests"] = json!([]);
            result["failed_tests_total"] = json!(0);
        }
        Err(failure) => return Err(failure.into()),
    }
    Ok(result)
}
