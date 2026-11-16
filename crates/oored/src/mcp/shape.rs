//! Small versions of API answers. Agents pay for every token they read, so
//! tool results keep the fields that answer "what is it and what state is
//! it in", and leave out large ones such as `config_snapshot`.

use serde_json::{Map, Value, json};

/// Copies the named fields that are present and not null.
fn pick(source: &Value, fields: &[&str]) -> Map<String, Value> {
    let mut out = Map::new();
    for field in fields {
        if let Some(value) = source.get(*field)
            && !value.is_null()
        {
            out.insert((*field).to_string(), value.clone());
        }
    }
    out
}

pub fn is_terminal(status: &str) -> bool {
    matches!(
        status,
        "succeeded" | "failed" | "canceled" | "timed_out" | "expired"
    )
}

pub fn app(project: &Value) -> Value {
    let mut out = pick(
        project,
        &[
            "id",
            "name",
            "platforms",
            "repository_full_name",
            "default_branch",
            "live_version",
            "last_build_at",
            "current_user_role",
        ],
    );
    if project.get("is_favorite").and_then(Value::as_bool) == Some(true) {
        out.insert("favorite".into(), json!(true));
    }
    Value::Object(out)
}

pub fn pipeline(pipeline: &Value) -> Value {
    let mut out = pick(
        pipeline,
        &[
            "id",
            "project_id",
            "name",
            "workflow",
            "config_path",
            "enabled",
        ],
    );
    if let Some(health) = pipeline.get("health").filter(|v| !v.is_null()) {
        out.insert("health".into(), health.clone());
    }
    Value::Object(out)
}

/// The step that is running, or the first that failed.
fn step_of_interest(build: &Value) -> Option<Value> {
    let steps = build.get("step_results")?.as_array()?;
    steps
        .iter()
        .find(|step| step.get("status").and_then(Value::as_str) == Some("failed"))
        .or_else(|| {
            steps
                .iter()
                .find(|step| step.get("status").and_then(Value::as_str) == Some("running"))
        })
        .map(|step| Value::Object(pick(step, &["name", "status", "exit_code"])))
}

/// One build in a list.
pub fn build_row(build: &Value) -> Value {
    let mut out = pick(
        build,
        &[
            "id",
            "build_number",
            "status",
            "project_id",
            "pipeline_id",
            "branch",
            "commit_sha",
            "commit_title",
            "workflow",
            "trigger_type",
            "trigger_actor",
            "created_at",
            "started_at",
            "finished_at",
        ],
    );
    if let Some(context) = build.get("context") {
        for (from, to) in [
            ("project_name", "app_name"),
            ("pipeline_name", "pipeline_name"),
        ] {
            if let Some(value) = context.get(from).filter(|v| !v.is_null()) {
                out.insert(to.into(), value.clone());
            }
        }
    }
    if let Some(sha) = out.get("commit_sha").and_then(Value::as_str) {
        let short: String = sha.chars().take(12).collect();
        out.insert("commit_sha".into(), json!(short));
    }
    if let Some(step) = step_of_interest(build) {
        let key = if step.get("status").and_then(Value::as_str) == Some("failed") {
            "failed_step"
        } else {
            "current_step"
        };
        out.insert(key.into(), step);
    }
    Value::Object(out)
}

/// One build in full, still without the config snapshot.
pub fn build_detail(build: &Value) -> Value {
    let mut out = match build_row(build) {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    for field in [
        "commit_author",
        "exit_code",
        "queued_at",
        "updated_at",
        "rehearsal",
        "changelog",
        "store_notes",
        "source_build_id",
        "runner_id",
        "approval",
        "release_outcome",
        "superseded_by",
        "runner_policy_block_reason",
        "retry_steps",
    ] {
        if let Some(value) = build.get(field)
            && !value.is_null()
            && value != &json!(false)
        {
            out.insert(field.into(), value.clone());
        }
    }
    if let Some(steps) = build.get("step_results").and_then(Value::as_array) {
        let steps: Vec<Value> = steps
            .iter()
            .map(|step| Value::Object(pick(step, &["name", "status", "exit_code", "duration_ms"])))
            .collect();
        out.insert("steps".into(), json!(steps));
    }
    if let Some(runner) = build
        .get("context")
        .and_then(|context| context.get("runner_name"))
        .filter(|v| !v.is_null())
    {
        out.insert("runner_name".into(), runner.clone());
    }
    Value::Object(out)
}

pub fn artifact(artifact: &Value) -> Value {
    Value::Object(pick(
        artifact,
        &[
            "id",
            "name",
            "artifact_type",
            "file_size",
            "state",
            "expires_at",
        ],
    ))
}

pub fn release(release: &Value) -> Value {
    Value::Object(pick(
        release,
        &[
            "id",
            "project_id",
            "project_name",
            "platform",
            "store",
            "track",
            "environment_name",
            "state",
            "state_reason",
            "store_status",
            "store_error",
            "version_name",
            "version_code",
            "build_id",
            "build_number",
            "rollout_fraction",
            "next_fraction",
            "next_step_at",
            "awaiting_approval",
            "health",
            "crash_rate",
            "allowed_actions",
            "console_url",
            "updated_at",
        ],
    ))
}

pub fn tester_group(group: &Value) -> Value {
    let mut out = pick(group, &["id", "name"]);
    if let Some(members) = group.get("members").and_then(Value::as_array) {
        out.insert("member_count".into(), json!(members.len()));
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_rows_drop_the_config_snapshot_and_name_the_failed_step() {
        let build = json!({
            "id": "b1",
            "build_number": 7,
            "status": "failed",
            "config_snapshot": {"huge": "x".repeat(10_000)},
            "commit_sha": "0123456789abcdef0123",
            "context": {"project_name": "Kite", "pipeline_name": "Android"},
            "step_results": [
                {"name": "Install", "status": "succeeded", "duration_ms": 10},
                {"name": "Build", "status": "failed", "exit_code": 1, "duration_ms": 20}
            ]
        });
        let row = build_row(&build);
        assert!(row.get("config_snapshot").is_none());
        assert_eq!(row["commit_sha"], "0123456789ab");
        assert_eq!(row["app_name"], "Kite");
        assert_eq!(row["failed_step"]["name"], "Build");
        let detail = build_detail(&build);
        assert!(detail.get("config_snapshot").is_none());
        assert_eq!(detail["steps"].as_array().unwrap().len(), 2);
    }
}
