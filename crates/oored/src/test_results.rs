//! Test results reported by runners and the Tests view of a build.

use std::collections::HashSet;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use oore_contract::{
    ApiError, BuildTestFailure, BuildTestResultsResponse, BuildTestSummary, OkResponse,
    RunnerTestReport, TestRunOutcome,
};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use tracing::error;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::logs::require_build_log_read;
use crate::runners::RunnerAuth;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

/// Failed tests kept per build. Totals still count every failure.
const MAX_FAILURES_PER_BUILD: i64 = 500;
/// Recent test-reporting builds of a pipeline used for history and flakiness.
pub const HISTORY_WINDOW: i64 = 20;
const MAX_NAME_BYTES: usize = 1024;
const MAX_MESSAGE_BYTES: usize = 4096;
const MAX_STACK_TRACE_BYTES: usize = 8192;

fn store_error(message: &'static str, error: sqlx::Error) -> (StatusCode, Json<ApiError>) {
    error!(error = %error, "{message}");
    api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", message)
}

fn truncate(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn test_key(suite: &str, name: &str) -> String {
    format!("{suite}\u{1f}{name}")
}

/// A test is flaky when it failed, then passed, then failed again: at least
/// two separate failure streaks. A single streak is a regression or a fix.
fn is_flaky(failed_oldest_first: &[bool]) -> bool {
    let mut streaks = 0;
    let mut previous = false;
    for &failed in failed_oldest_first {
        if failed && !previous {
            streaks += 1;
        }
        previous = failed;
    }
    streaks >= 2
}

/// `POST /v1/runners/{runner_id}/jobs/{job_id}/test-results`
pub async fn report_job_test_results(
    State(state): State<Arc<AppState>>,
    Path((runner_id, job_id)): Path<(String, String)>,
    runner_auth: RunnerAuth,
    Json(report): Json<RunnerTestReport>,
) -> ApiResult<OkResponse> {
    if runner_auth.runner_id != runner_id {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "runner_mismatch",
            "Runner token does not match the requested runner ID",
        ));
    }
    let row = sqlx::query("SELECT runner_id, status, pipeline_id FROM builds WHERE id = ?1")
        .bind(&job_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| store_error("Failed to fetch build", e))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Build not found"))?;
    let build_runner_id: Option<String> = row.get("runner_id");
    if build_runner_id.as_deref() != Some(runner_id.as_str()) {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "runner_mismatch",
            "This build is not assigned to your runner",
        ));
    }
    let status: String = row.get("status");
    if status != "running" {
        return Err(api_err(
            StatusCode::CONFLICT,
            "job_not_active",
            "Test results can only be reported while the build is running",
        ));
    }
    let pipeline_id: String = row.get("pipeline_id");
    record_report(&state.db, &job_id, &pipeline_id, &report)
        .await
        .map_err(|e| store_error("Failed to record test results", e))?;
    Ok(Json(OkResponse { ok: true }))
}

async fn record_report(
    pool: &SqlitePool,
    build_id: &str,
    pipeline_id: &str,
    report: &RunnerTestReport,
) -> Result<(), sqlx::Error> {
    let now = now_unix();
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO build_test_summaries \
         (build_id, pipeline_id, total, passed, failed, skipped, duration_ms, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8) \
         ON CONFLICT(build_id) DO UPDATE SET \
         total = total + excluded.total, passed = passed + excluded.passed, \
         failed = failed + excluded.failed, skipped = skipped + excluded.skipped, \
         duration_ms = duration_ms + excluded.duration_ms, updated_at = excluded.updated_at",
    )
    .bind(build_id)
    .bind(pipeline_id)
    .bind(report.total.max(0))
    .bind(report.passed.max(0))
    .bind(report.failed.max(0))
    .bind(report.skipped.max(0))
    .bind(report.duration_ms.max(0))
    .bind(now)
    .execute(&mut *tx)
    .await?;

    let stored: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM build_test_failures WHERE build_id = ?1")
            .bind(build_id)
            .fetch_one(&mut *tx)
            .await?;
    let room = (MAX_FAILURES_PER_BUILD - stored).max(0) as usize;
    for failure in report.failures.iter().take(room) {
        let suite = truncate(&failure.suite, MAX_NAME_BYTES);
        let name = truncate(&failure.name, MAX_NAME_BYTES);
        sqlx::query(
            "INSERT OR IGNORE INTO build_test_failures \
             (build_id, test_key, suite, name, step, message, stack_trace, duration_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(build_id)
        .bind(test_key(&suite, &name))
        .bind(&suite)
        .bind(&name)
        .bind(truncate(&report.step, MAX_NAME_BYTES))
        .bind(
            failure
                .message
                .as_deref()
                .map(|m| truncate(m, MAX_MESSAGE_BYTES)),
        )
        .bind(
            failure
                .stack_trace
                .as_deref()
                .map(|s| truncate(s, MAX_STACK_TRACE_BYTES)),
        )
        .bind(failure.duration_ms)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

/// `GET /v1/builds/{build_id}/tests`
pub async fn get_build_test_results(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
) -> ApiResult<BuildTestResultsResponse> {
    require_build_log_read(&state.db, &auth.0, &build_id).await?;
    load_build_test_results(&state.db, &build_id)
        .await
        .map(Json)
        .map_err(|e| store_error("Failed to load test results", e))
}

async fn load_build_test_results(
    pool: &SqlitePool,
    build_id: &str,
) -> Result<BuildTestResultsResponse, sqlx::Error> {
    let Some(row) = sqlx::query(
        "SELECT s.total, s.passed, s.failed, s.skipped, s.duration_ms, s.pipeline_id, b.build_number \
         FROM build_test_summaries s JOIN builds b ON b.id = s.build_id WHERE s.build_id = ?1",
    )
    .bind(build_id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(BuildTestResultsResponse {
            summary: None,
            failures: Vec::new(),
            history_window: HISTORY_WINDOW,
        });
    };
    let summary = BuildTestSummary {
        total: row.get("total"),
        passed: row.get("passed"),
        failed: row.get("failed"),
        skipped: row.get("skipped"),
        duration_ms: row.get("duration_ms"),
    };
    let pipeline_id: String = row.get("pipeline_id");
    let build_number: i64 = row.get("build_number");

    let failure_rows = sqlx::query(
        "SELECT test_key, suite, name, step, message, stack_trace, duration_ms \
         FROM build_test_failures WHERE build_id = ?1 ORDER BY suite, name",
    )
    .bind(build_id)
    .fetch_all(pool)
    .await?;

    // Oldest first, ending with this build.
    let mut history_builds: Vec<(String, i64)> = sqlx::query(
        "SELECT s.build_id, b.build_number FROM build_test_summaries s \
         JOIN builds b ON b.id = s.build_id \
         WHERE s.pipeline_id = ?1 AND b.build_number <= ?2 \
         ORDER BY b.build_number DESC LIMIT ?3",
    )
    .bind(&pipeline_id)
    .bind(build_number)
    .bind(HISTORY_WINDOW)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|row| (row.get("build_id"), row.get("build_number")))
    .collect();
    history_builds.reverse();

    let mut failed_runs: HashSet<(String, String)> = HashSet::new();
    if !failure_rows.is_empty() {
        let mut query: QueryBuilder<Sqlite> = QueryBuilder::new(
            "SELECT build_id, test_key FROM build_test_failures WHERE build_id IN (",
        );
        let mut ids = query.separated(", ");
        for (id, _) in &history_builds {
            ids.push_bind(id);
        }
        query.push(") AND test_key IN (");
        let mut keys = query.separated(", ");
        for row in &failure_rows {
            keys.push_bind(row.get::<String, _>("test_key"));
        }
        query.push(")");
        for row in query.build().fetch_all(pool).await? {
            failed_runs.insert((row.get("build_id"), row.get("test_key")));
        }
    }

    let failures = failure_rows
        .iter()
        .map(|row| {
            let key: String = row.get("test_key");
            let history: Vec<TestRunOutcome> = history_builds
                .iter()
                .map(|(id, number)| TestRunOutcome {
                    build_id: id.clone(),
                    build_number: *number,
                    status: if failed_runs.contains(&(id.clone(), key.clone())) {
                        "failed"
                    } else {
                        "passed"
                    }
                    .to_string(),
                })
                .collect();
            let failed: Vec<bool> = history.iter().map(|run| run.status == "failed").collect();
            BuildTestFailure {
                suite: row.get("suite"),
                name: row.get("name"),
                step: row.get("step"),
                message: row.get("message"),
                stack_trace: row.get("stack_trace"),
                duration_ms: row.get("duration_ms"),
                flaky: is_flaky(&failed),
                history,
            }
        })
        .collect();

    Ok(BuildTestResultsResponse {
        summary: Some(summary),
        failures,
        history_window: HISTORY_WINDOW,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use oore_contract::RunnerTestFailure;

    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for statement in [
            "CREATE TABLE builds (id TEXT PRIMARY KEY, pipeline_id TEXT, build_number INTEGER)",
            include_str!("../migrations/053_build_test_results.sql"),
        ] {
            sqlx::raw_sql(statement).execute(&pool).await.unwrap();
        }
        pool
    }

    fn report(step: &str, total: i64, failing: &[&str]) -> RunnerTestReport {
        RunnerTestReport {
            step: step.to_string(),
            total,
            passed: total - failing.len() as i64,
            failed: failing.len() as i64,
            skipped: 0,
            duration_ms: 100,
            failures: failing
                .iter()
                .map(|name| RunnerTestFailure {
                    suite: "test/app_test.dart".to_string(),
                    name: name.to_string(),
                    message: Some(format!("{name} broke")),
                    ..Default::default()
                })
                .collect(),
        }
    }

    async fn build(pool: &SqlitePool, number: i64, pipeline: &str, failing: Option<&[&str]>) {
        let id = format!("b{number}");
        sqlx::query("INSERT INTO builds VALUES (?1, ?2, ?3)")
            .bind(&id)
            .bind(pipeline)
            .bind(number)
            .execute(pool)
            .await
            .unwrap();
        if let Some(failing) = failing {
            record_report(pool, &id, pipeline, &report("build-1", 10, failing))
                .await
                .unwrap();
        }
    }

    #[test]
    fn flaky_needs_two_failure_streaks() {
        assert!(!is_flaky(&[false, false, true, true]));
        assert!(!is_flaky(&[false, true, true, false]));
        assert!(is_flaky(&[true, false, true]));
        assert!(is_flaky(&[false, true, false, false, true]));
    }

    #[tokio::test]
    async fn build_without_reports_has_no_summary() {
        let pool = pool().await;
        build(&pool, 1, "pl", None).await;
        let results = load_build_test_results(&pool, "b1").await.unwrap();
        assert_eq!(results.summary, None);
        assert!(results.failures.is_empty());
    }

    #[tokio::test]
    async fn reports_from_several_steps_add_up() {
        let pool = pool().await;
        build(&pool, 1, "pl", Some(&["a"])).await;
        record_report(&pool, "b1", "pl", &report("build-2", 5, &["b"]))
            .await
            .unwrap();
        let results = load_build_test_results(&pool, "b1").await.unwrap();
        let summary = results.summary.unwrap();
        assert_eq!((summary.total, summary.failed, summary.passed), (15, 2, 13));
        let names: Vec<_> = results.failures.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["a", "b"]);
        assert_eq!(results.failures[1].step, "build-2");
        assert_eq!(results.failures[0].message.as_deref(), Some("a broke"));
    }

    #[tokio::test]
    async fn intermittent_failure_is_flaky_and_regression_is_not() {
        let pool = pool().await;
        build(&pool, 1, "pl", Some(&["intermittent"])).await;
        build(&pool, 2, "pl", Some(&[])).await;
        // A build that reported no tests is left out of the history.
        build(&pool, 3, "pl", None).await;
        // Other pipelines do not count.
        build(&pool, 4, "other", Some(&["intermittent"])).await;
        build(&pool, 5, "pl", Some(&["intermittent", "regression"])).await;
        build(&pool, 6, "pl", Some(&["regression"])).await;
        build(&pool, 7, "pl", Some(&["intermittent", "regression"])).await;

        let results = load_build_test_results(&pool, "b7").await.unwrap();
        let intermittent = &results.failures[0];
        assert_eq!(intermittent.name, "intermittent");
        assert!(intermittent.flaky);
        let runs: Vec<_> = intermittent
            .history
            .iter()
            .map(|run| (run.build_number, run.status.as_str()))
            .collect();
        assert_eq!(
            runs,
            [
                (1, "failed"),
                (2, "passed"),
                (5, "failed"),
                (6, "passed"),
                (7, "failed")
            ]
        );
        assert_eq!(results.failures[1].name, "regression");
        assert!(!results.failures[1].flaky);

        // History stops at the build being viewed.
        let earlier = load_build_test_results(&pool, "b5").await.unwrap();
        assert_eq!(earlier.failures[0].history.len(), 3);
    }

    #[tokio::test]
    async fn stored_failures_are_capped_per_build() {
        let pool = pool().await;
        build(&pool, 1, "pl", None).await;
        let names: Vec<String> = (0..MAX_FAILURES_PER_BUILD + 5)
            .map(|i| format!("t{i}"))
            .collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let big = report("build-1", refs.len() as i64, &refs);
        record_report(&pool, "b1", "pl", &big).await.unwrap();
        let results = load_build_test_results(&pool, "b1").await.unwrap();
        assert_eq!(results.failures.len() as i64, MAX_FAILURES_PER_BUILD);
        assert_eq!(results.summary.unwrap().failed, MAX_FAILURES_PER_BUILD + 5);
    }
}
