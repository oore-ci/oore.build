//! Why a build failed, in one call: the failed step, the line that explains
//! it, the end of that step's log and the failed tests. Built for agents and
//! the CLI, which would otherwise need four calls and the whole log.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use oore_contract::{ApiError, BuildDiagnosis, BuildDiagnosisStep, BuildDiagnosisTest, StepResult};
use serde::Deserialize;
use sqlx::{Row, SqlitePool};
use tracing::error;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::logs::{load_step_log_rows, require_build_log_read};
use crate::util::api_err;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

const DEFAULT_TAIL: usize = 40;
const MAX_TAIL: i64 = 200;
const MAX_TESTS: i64 = 20;
const MAX_LINE_CHARS: usize = 400;
const MAX_MESSAGE_CHARS: usize = 500;

#[derive(Debug, Deserialize)]
pub struct DiagnosisQuery {
    /// How many log lines to return, 1 to 200 (default 40).
    pub tail: Option<i64>,
}

/// Labels for the steps the runner adds around a pipeline's own steps. Keep
/// in step with `RUNNER_STEP_LABELS` in `apps/web/src/lib/build-steps.ts`.
const RUNNER_STEP_LABELS: &[(&str, &str)] = &[
    ("checkout", "Check out code"),
    ("vm-start", "Start VM"),
    ("flutter-version", "Set up Flutter"),
    ("android-sign", "Verify Android signing"),
    ("ios-sign", "Sign iOS app"),
    ("analyze", "Analyze code"),
    ("artifacts", "Collect artifacts"),
    ("publish-play", "Upload to Google Play"),
    ("publish-app-store", "Upload to App Store Connect"),
    ("rehearse-play", "Rehearse Google Play release"),
    ("rehearse-app-store", "Rehearse App Store submission"),
];

const STAGE_LABELS: &[(&str, &str)] = &[
    ("pre_build", "Pre-build"),
    ("build", "Build"),
    ("post_build", "Post-build"),
];

/// `pre_build-1` as (`pre_build`, `1`).
fn numbered_step(name: &str) -> Option<(&str, &str)> {
    let (stage, index) = name.rsplit_once('-')?;
    (STAGE_LABELS.iter().any(|(known, _)| *known == stage)
        && !index.is_empty()
        && index.bytes().all(|b| b.is_ascii_digit()))
    .then_some((stage, index))
}

/// The label people see for a step, as the web app shows it.
pub(crate) fn step_label<'a>(name: &str, step_names: impl IntoIterator<Item = &'a str>) -> String {
    if let Some((_, label)) = RUNNER_STEP_LABELS.iter().find(|(id, _)| *id == name) {
        return (*label).to_string();
    }
    let Some((stage, index)) = numbered_step(name) else {
        return name.to_string();
    };
    let stage_label = STAGE_LABELS
        .iter()
        .find(|(id, _)| *id == stage)
        .map_or(stage, |(_, label)| label);
    let has_sibling = step_names
        .into_iter()
        .any(|other| other != name && numbered_step(other).is_some_and(|(s, _)| s == stage));
    if has_sibling || index != "1" {
        format!("{stage_label} {index}")
    } else {
        stage_label.to_string()
    }
}

fn shortened(text: &str, max: usize) -> String {
    let text = text.trim_end();
    if text.chars().count() <= max {
        return text.to_string();
    }
    text.chars().take(max).collect::<String>() + "…"
}

/// `GET /v1/builds/{build_id}/diagnosis`
pub async fn get_build_diagnosis(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
    Query(params): Query<DiagnosisQuery>,
) -> ApiResult<BuildDiagnosis> {
    let tail = match params.tail {
        Some(tail) if !(1..=MAX_TAIL).contains(&tail) => {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "tail must be between 1 and 200",
            ));
        }
        Some(tail) => tail as usize,
        None => DEFAULT_TAIL,
    };
    require_build_log_read(&state.db, &auth.0, &build_id).await?;
    load_diagnosis(&state.db, &build_id, tail)
        .await
        .map_err(|e| {
            error!(error = %e, build_id = %build_id, "failed to diagnose the build");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to diagnose the build",
            )
        })?
        .map(Json)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Build not found"))
}

pub(crate) async fn load_diagnosis(
    pool: &SqlitePool,
    build_id: &str,
    tail: usize,
) -> anyhow::Result<Option<BuildDiagnosis>> {
    let Some(row) =
        sqlx::query("SELECT build_number, status, step_results FROM builds WHERE id = ?1")
            .bind(build_id)
            .fetch_optional(pool)
            .await?
    else {
        return Ok(None);
    };
    let build_number: i64 = row.get("build_number");
    let status: String = row.get("status");
    let steps: Vec<StepResult> = row
        .get::<Option<String>, _>("step_results")
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    let failed = matches!(status.as_str(), "failed" | "timed_out");
    let failed_step = failed
        .then(|| steps.iter().rev().find(|step| step.status == "failed"))
        .flatten()
        .map(|step| BuildDiagnosisStep {
            name: step.name.clone(),
            label: step_label(&step.name, steps.iter().map(|s| s.name.as_str())),
            exit_code: step.exit_code,
        });

    let url = crate::commit_checks::frontend_base_url(pool)
        .await
        .map(|mut base| {
            base.set_path(&format!("/builds/{build_id}"));
            base.set_query(None);
            base.set_fragment(None);
            String::from(base)
        });

    if !failed {
        return Ok(Some(BuildDiagnosis {
            build_id: build_id.to_string(),
            build_number,
            status,
            failed_step: None,
            hint: None,
            log_tail: Vec::new(),
            log_tail_truncated: false,
            failed_tests: Vec::new(),
            failed_tests_total: 0,
            url,
        }));
    }

    let hint = crate::build_failure_hint::load_failure_hint(pool, build_id).await?;

    let rows = load_step_log_rows(pool, build_id).await?;
    let step_name = failed_step.as_ref().map(|step| step.name.as_str());
    // When the step's start marker was dropped from an oversized log, its
    // lines carry no step; the end of the whole log is the best answer then.
    let step_seen =
        step_name.is_some_and(|name| rows.iter().any(|r| r.step.as_deref() == Some(name)));
    let lines: Vec<&str> = rows
        .iter()
        .filter(|row| !step_seen || row.step.as_deref() == step_name)
        .flat_map(|row| row.content.lines())
        .collect();
    let skip = lines.len().saturating_sub(tail);
    let log_tail = lines[skip..]
        .iter()
        .map(|line| shortened(line, MAX_LINE_CHARS))
        .collect();

    let failed_tests_total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM build_test_failures WHERE build_id = ?1")
            .bind(build_id)
            .fetch_one(pool)
            .await?;
    let reported_failed: Option<i64> =
        sqlx::query_scalar("SELECT failed FROM build_test_summaries WHERE build_id = ?1")
            .bind(build_id)
            .fetch_optional(pool)
            .await?;
    let failed_tests = sqlx::query(
        "SELECT suite, name, message FROM build_test_failures \
         WHERE build_id = ?1 ORDER BY suite, name LIMIT ?2",
    )
    .bind(build_id)
    .bind(MAX_TESTS)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|row| {
        let suite: String = row.get("suite");
        let name: String = row.get("name");
        BuildDiagnosisTest {
            name: if suite.is_empty() {
                name
            } else {
                format!("{suite}: {name}")
            },
            message: row
                .get::<Option<String>, _>("message")
                .map(|message| shortened(&message, MAX_MESSAGE_CHARS)),
        }
    })
    .collect();

    Ok(Some(BuildDiagnosis {
        build_id: build_id.to_string(),
        build_number,
        status,
        failed_step,
        hint,
        log_tail,
        log_tail_truncated: skip > 0,
        failed_tests,
        failed_tests_total: reported_failed.unwrap_or(0).max(failed_tests_total),
        url,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_match_the_web_app() {
        assert_eq!(step_label("checkout", []), "Check out code");
        assert_eq!(step_label("build-1", ["build-1"]), "Build");
        assert_eq!(step_label("build-1", ["build-1", "build-2"]), "Build 1");
        assert_eq!(step_label("pre_build-2", ["pre_build-2"]), "Pre-build 2");
        assert_eq!(step_label("Run tests", ["Run tests"]), "Run tests");
    }

    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO users (id, email, oidc_subject, role, created_at, updated_at) \
             VALUES ('u1', 'a@example.com', 'sub', 'owner', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO projects (id, name, created_by, created_at, updated_at) \
             VALUES ('p1', 'App', 'u1', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO pipelines (id, project_id, name, created_at, updated_at) \
             VALUES ('pl1', 'p1', 'Main', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    fn marker(name: &str) -> String {
        format!(
            "[oore-step] {}",
            serde_json::json!({ "event": "start", "name": name })
        )
    }

    async fn insert_build(pool: &SqlitePool, id: &str, status: &str, steps: &str, log: &[String]) {
        sqlx::query(
            "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
             step_results, queued_at, created_at, updated_at) \
             VALUES (?1, 'p1', 'pl1', 7, ?2, 'manual', ?3, 0, 0, 0)",
        )
        .bind(id)
        .bind(status)
        .bind(steps)
        .execute(pool)
        .await
        .unwrap();
        for (sequence, line) in log.iter().enumerate() {
            sqlx::query(
                "INSERT INTO build_logs (id, build_id, sequence, content, stream, created_at) \
                 VALUES (?1, ?2, ?3, ?4, 'stdout', 0)",
            )
            .bind(format!("{id}-{sequence}"))
            .bind(id)
            .bind(sequence as i64)
            .bind(line)
            .execute(pool)
            .await
            .unwrap();
        }
    }

    #[tokio::test]
    async fn a_failed_build_names_the_step_and_its_last_lines() {
        let pool = pool().await;
        let steps = serde_json::json!([
            { "name": "checkout", "status": "succeeded", "exit_code": 0, "started_at": 0, "finished_at": 1, "duration_ms": 1 },
            { "name": "build-1", "status": "failed", "exit_code": 1, "started_at": 1, "finished_at": 2, "duration_ms": 1 },
        ])
        .to_string();
        let mut log = vec![marker("checkout"), "cloned".to_string(), marker("build-1")];
        log.extend((0..50).map(|i| format!("compiling {i}")));
        log.push("lib/main.dart:3:1: Error: Expected ';'.".to_string());
        insert_build(&pool, "b1", "failed", &steps, &log).await;

        let diagnosis = load_diagnosis(&pool, "b1", 5).await.unwrap().unwrap();
        let step = diagnosis.failed_step.unwrap();
        assert_eq!(step.name, "build-1");
        assert_eq!(step.label, "Build");
        assert_eq!(step.exit_code, Some(1));
        assert_eq!(diagnosis.hint.unwrap().category, "dart");
        assert_eq!(diagnosis.log_tail.len(), 5);
        assert_eq!(
            diagnosis.log_tail.last().unwrap(),
            "lib/main.dart:3:1: Error: Expected ';'."
        );
        assert!(diagnosis.log_tail_truncated);
        assert!(!diagnosis.log_tail.iter().any(|line| line == "cloned"));
    }

    #[tokio::test]
    async fn a_passing_build_has_nothing_to_explain() {
        let pool = pool().await;
        insert_build(&pool, "b2", "succeeded", "[]", &["done".to_string()]).await;
        let diagnosis = load_diagnosis(&pool, "b2", 40).await.unwrap().unwrap();
        assert!(diagnosis.failed_step.is_none());
        assert!(diagnosis.log_tail.is_empty());
        assert_eq!(diagnosis.failed_tests_total, 0);
    }
}
