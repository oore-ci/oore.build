//! The example payloads on the "Outbound webhooks" docs page
//! (`apps/docs/content/docs/reference/webhooks.mdx`) must have the same
//! fields as the payloads this module sends, so the docs cannot drift.
//!
//! Each `json` block on the page with an `event` field is compared, key by
//! key and at every depth, with a payload built by the real code for the
//! same event. Values may differ; the set of keys may not.

use super::*;
use serde_json::Value;

fn docs_examples() -> Vec<Value> {
    let page = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/docs/content/docs/reference/webhooks.mdx");
    let text = std::fs::read_to_string(&page).unwrap();
    let mut examples = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if line.trim() == "```json" {
            let block: Vec<&str> = lines
                .by_ref()
                .take_while(|line| line.trim() != "```")
                .collect();
            let value: Value = serde_json::from_str(&block.join("\n")).unwrap_or_else(|error| {
                panic!("a json block on {} is invalid: {error}", page.display())
            });
            if value.get("event").is_some() {
                examples.push(value);
            }
        }
    }
    examples
}

/// Differences in keys between `docs` and `code`, with their paths. A key
/// that is `null` on one side may be an object on the other.
fn key_differences(docs: &Value, code: &Value, at: &str, out: &mut Vec<String>) {
    let (Some(docs), Some(code)) = (docs.as_object(), code.as_object()) else {
        return;
    };
    for key in docs.keys() {
        if !code.contains_key(key) {
            out.push(format!("{at}.{key} is in the docs but not sent"));
        }
    }
    for (key, value) in code {
        match docs.get(key) {
            None => out.push(format!("{at}.{key} is sent but not in the docs")),
            Some(documented) => key_differences(documented, value, &format!("{at}.{key}"), out),
        }
    }
}

async fn build_payload(event_type: &str, to_status: &str) -> Value {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO users (id, email, oidc_subject, created_at, updated_at) \
         VALUES ('u1', 'dev@example.com', 'sub', 0, 0)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO projects (id, name, created_by, created_at, updated_at) \
         VALUES ('p1', 'FlutterShop', 'u1', 0, 0)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO pipelines (id, project_id, name, created_at, updated_at) \
         VALUES ('pl1', 'p1', 'Release', 0, 0)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
             trigger_actor, branch, commit_sha, config_snapshot, queued_at, created_at, updated_at) \
         VALUES ('b1', 'p1', 'pl1', 142, ?1, 'webhook', 'octocat', 'main', 'abc', '{}', 0, 0, 0)",
    )
    .bind(to_status)
    .execute(&pool)
    .await
    .unwrap();
    let event = BuildStateEvent {
        build_id: "b1".to_string(),
        from_status: None,
        to_status: to_status.to_string(),
        actor: None,
        reason: None,
        timestamp: 0,
    };
    build_notification(&pool, &event, event_type)
        .await
        .unwrap()
        .expect("build notification")
        .payload
}

async fn code_payload(event: &str) -> Value {
    if event == "build.qa_feedback" {
        let mut payload = build_payload(QA_FEEDBACK_EVENT, "succeeded").await;
        payload["event"] = serde_json::json!("build.qa_feedback");
        payload["feedback"] = feedback_payload(&oore_contract::BuildFeedback {
            id: "f1".to_string(),
            build_id: "b1".to_string(),
            author_email: "tester@example.com".to_string(),
            author_name: None,
            message: "note".to_string(),
            device: None,
            os_version: None,
            app_version: None,
            artifact_id: None,
            has_screenshot: false,
            created_at: 0,
        });
        return payload;
    }
    if let Some(status) = event.strip_prefix("build.") {
        return build_payload(status, status).await;
    }
    if let Some(event_type) = event.strip_prefix("release.") {
        let release = ReleaseEvent {
            kind: ReleaseEventKind::RolloutHalted,
            project_id: "p1".to_string(),
            release_id: "r1".to_string(),
            build_id: None,
            version_name: None,
            version_code: None,
            track: None,
            rollout_percent: None,
            actor: None,
            reason: None,
            state: None,
            store_status: None,
            console_url: None,
            timestamp: 0,
        };
        return release_notification_payload(&release, event_type, Some("FlutterShop"));
    }
    if event == "signing.expiring" {
        let expiry = crate::signing_expiry::Expiry {
            key: "pipeline-profile:1".to_string(),
            project_id: "p1".to_string(),
            project_name: "FlutterShop".to_string(),
            what: "Provisioning profile".to_string(),
            holder: "Release".to_string(),
            expires_at: 1_000,
            path: "/projects/p1".to_string(),
        };
        return signing_notification_payload(&expiry, crate::signing_expiry::Band::Week, 0);
    }
    if let Some(status) = event.strip_prefix("runner.") {
        return runner_notification_payload(&RunnerStateEvent {
            runner_id: "r1".to_string(),
            runner_name: "mac".to_string(),
            from_status: "online".to_string(),
            to_status: status.to_string(),
            timestamp: 0,
        });
    }
    panic!("the docs show an event the test cannot build: {event}");
}

#[tokio::test]
async fn webhook_docs_examples_match_the_payloads_sent() {
    let examples = docs_examples();
    let events: Vec<&str> = examples
        .iter()
        .map(|example| example["event"].as_str().unwrap())
        .collect();
    for family in [
        "build.",
        "build.qa_feedback",
        "release.",
        "signing.",
        "runner.",
    ] {
        assert!(
            events.iter().any(|event| event.starts_with(family)),
            "the webhooks page has no {family} example"
        );
    }

    let mut differences = Vec::new();
    for example in &examples {
        let event = example["event"].as_str().unwrap();
        let code = code_payload(event).await;
        assert_eq!(code["event"], example["event"], "{event}: event name");
        let mut found = Vec::new();
        key_differences(example, &code, event, &mut found);
        differences.extend(found);
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

#[test]
fn webhook_docs_list_every_release_event() {
    let page = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/docs/content/docs/reference/webhooks.mdx"),
    )
    .unwrap();
    let kinds = [
        ReleaseEventKind::Created,
        ReleaseEventKind::Uploaded,
        ReleaseEventKind::Blocked,
        ReleaseEventKind::Submitted,
        ReleaseEventKind::StoreApproved,
        ReleaseEventKind::Rejected,
        ReleaseEventKind::Live,
        ReleaseEventKind::RolloutStepRaised,
        ReleaseEventKind::RolloutHalted,
        ReleaseEventKind::RolloutCompleted,
        ReleaseEventKind::ApprovalNeeded,
        ReleaseEventKind::Failed,
        ReleaseEventKind::Superseded,
        ReleaseEventKind::RolloutPaused,
        ReleaseEventKind::RolloutResumed,
        ReleaseEventKind::Approved,
        ReleaseEventKind::Updated,
    ];
    for kind in kinds {
        if let Some(name) = release_event_type(kind) {
            assert!(
                page.contains(&format!("`release.{name}`")),
                "the webhooks page does not list release.{name}"
            );
        }
    }
}
