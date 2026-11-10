//! Trends across builds for Insights: tests that fail and then pass on the
//! same commit, and steps that got slower.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use oore_contract::{ApiError, BuildTrendsResponse, FlakyTest, SlowerStep, StepResult};
use serde::Deserialize;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use tracing::error;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

const DAY: i64 = 86_400;
const WINDOWS: [i64; 4] = [7, 14, 30, 90];
const MAX_FLAKY: usize = 20;
const MAX_SLOWER: usize = 10;
/// Succeeded builds each half of the window needs before a step's median
/// counts.
const MIN_BUILDS_PER_HALF: usize = 3;
/// A step is slower when its median grew by at least this share...
const MIN_GROWTH: f64 = 0.25;
/// ...and by at least this many seconds.
const MIN_GROWTH_SECONDS: i64 = 30;
const MAX_BUILDS_SCANNED: i64 = 5_000;

#[derive(Debug, Deserialize)]
pub struct BuildTrendsQuery {
    pub days: Option<i64>,
}

/// Members see trends of their apps; owners and admins see every app.
fn push_scope(query: &mut QueryBuilder<Sqlite>, member_user_id: Option<&str>) {
    if let Some(user_id) = member_user_id {
        query.push(
            " AND EXISTS (SELECT 1 FROM project_members pm \
             WHERE pm.project_id = b.project_id AND pm.user_id = ",
        );
        query.push_bind(user_id.to_string());
        query.push(")");
    }
}

/// `GET /v1/builds/trends`
pub async fn get_build_trends(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<BuildTrendsQuery>,
) -> ApiResult<BuildTrendsResponse> {
    let days = params.days.unwrap_or(30);
    if !WINDOWS.contains(&days) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "days must be 7, 14, 30 or 90",
        ));
    }
    let privileged = auth.0.role == "owner" || auth.0.role == "admin";
    let member = (!privileged).then_some(auth.0.user_id.as_str());
    load_build_trends(&state.db, member, days, now_unix())
        .await
        .map(Json)
        .map_err(|e| {
            error!(error = %e, "Failed to load build trends");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load build trends",
            )
        })
}

pub(crate) async fn load_build_trends(
    pool: &SqlitePool,
    member_user_id: Option<&str>,
    days: i64,
    now: i64,
) -> sqlx::Result<BuildTrendsResponse> {
    let since = now - days * DAY;
    Ok(BuildTrendsResponse {
        window_days: days,
        flaky_tests: flaky_tests(pool, member_user_id, since).await?,
        slower_steps: slower_steps(pool, member_user_id, since, now).await?,
    })
}

async fn flaky_tests(
    pool: &SqlitePool,
    member_user_id: Option<&str>,
    since: i64,
) -> sqlx::Result<Vec<FlakyTest>> {
    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT f.test_key, f.suite, f.name, b.project_id, pr.name AS project_name, \
         b.pipeline_id, pl.name AS pipeline_name, b.id AS failed_id, \
         b.build_number AS failed_number, \
         (SELECT b2.id FROM builds b2 JOIN build_test_summaries s ON s.build_id = b2.id \
          WHERE b2.pipeline_id = b.pipeline_id AND b2.commit_sha = b.commit_sha \
          AND b2.created_at > b.created_at \
          AND NOT EXISTS (SELECT 1 FROM build_test_failures f2 \
                          WHERE f2.build_id = b2.id AND f2.test_key = f.test_key) \
          ORDER BY b2.created_at LIMIT 1) AS passed_id \
         FROM build_test_failures f \
         JOIN builds b ON b.id = f.build_id \
         LEFT JOIN projects pr ON pr.id = b.project_id \
         LEFT JOIN pipelines pl ON pl.id = b.pipeline_id \
         WHERE b.commit_sha IS NOT NULL AND b.created_at >= ",
    );
    query.push_bind(since);
    push_scope(&mut query, member_user_id);
    query.push(" ORDER BY b.created_at");
    let rows = query.build().fetch_all(pool).await?;

    let mut by_test: HashMap<(String, String), FlakyTest> = HashMap::new();
    for row in rows {
        let Some(passed_id) = row.get::<Option<String>, _>("passed_id") else {
            continue;
        };
        let (passed_number, passed_at): (i64, i64) =
            sqlx::query_as("SELECT build_number, created_at FROM builds WHERE id = ?1")
                .bind(&passed_id)
                .fetch_one(pool)
                .await?;
        let pipeline_id: String = row.get("pipeline_id");
        let key = (pipeline_id.clone(), row.get::<String, _>("test_key"));
        let entry = by_test.entry(key).or_insert_with(|| FlakyTest {
            project_id: row.get("project_id"),
            project_name: row
                .get::<Option<String>, _>("project_name")
                .unwrap_or_default(),
            pipeline_id,
            pipeline_name: row
                .get::<Option<String>, _>("pipeline_name")
                .unwrap_or_default(),
            suite: row.get("suite"),
            name: row.get("name"),
            occurrences: 0,
            failed_build_id: String::new(),
            failed_build_number: 0,
            passed_build_id: String::new(),
            passed_build_number: 0,
            last_seen_at: 0,
        });
        entry.occurrences += 1;
        // Rows come oldest first, so the last pair is the latest.
        entry.failed_build_id = row.get("failed_id");
        entry.failed_build_number = row.get("failed_number");
        entry.passed_build_id = passed_id;
        entry.passed_build_number = passed_number;
        entry.last_seen_at = passed_at;
    }
    let mut flaky: Vec<FlakyTest> = by_test.into_values().collect();
    flaky.sort_by(|a, b| {
        b.occurrences
            .cmp(&a.occurrences)
            .then(b.last_seen_at.cmp(&a.last_seen_at))
    });
    flaky.truncate(MAX_FLAKY);
    Ok(flaky)
}

fn median(values: &mut [i64]) -> i64 {
    values.sort_unstable();
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[mid - 1] + values[mid]) / 2
    } else {
        values[mid]
    }
}

#[derive(Default)]
struct StepTimes {
    project_id: String,
    project_name: String,
    pipeline_name: String,
    before: Vec<i64>,
    after: Vec<i64>,
}

async fn slower_steps(
    pool: &SqlitePool,
    member_user_id: Option<&str>,
    since: i64,
    now: i64,
) -> sqlx::Result<Vec<SlowerStep>> {
    let midpoint = since + (now - since) / 2;
    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT b.project_id, pr.name AS project_name, b.pipeline_id, pl.name AS pipeline_name, \
         b.finished_at, b.step_results \
         FROM builds b \
         LEFT JOIN projects pr ON pr.id = b.project_id \
         LEFT JOIN pipelines pl ON pl.id = b.pipeline_id \
         WHERE b.status = 'succeeded' AND b.step_results IS NOT NULL AND b.finished_at >= ",
    );
    query.push_bind(since);
    push_scope(&mut query, member_user_id);
    query.push(" ORDER BY b.finished_at DESC LIMIT ");
    query.push_bind(MAX_BUILDS_SCANNED);
    let rows = query.build().fetch_all(pool).await?;

    let mut times: HashMap<(String, String), StepTimes> = HashMap::new();
    for row in rows {
        let Ok(steps) =
            serde_json::from_str::<Vec<StepResult>>(&row.get::<String, _>("step_results"))
        else {
            continue;
        };
        let finished_at: i64 = row.get("finished_at");
        let pipeline_id: String = row.get("pipeline_id");
        for step in steps.into_iter().filter(|step| step.status == "succeeded") {
            let entry = times
                .entry((pipeline_id.clone(), step.name))
                .or_insert_with(|| StepTimes {
                    project_id: row.get("project_id"),
                    project_name: row
                        .get::<Option<String>, _>("project_name")
                        .unwrap_or_default(),
                    pipeline_name: row
                        .get::<Option<String>, _>("pipeline_name")
                        .unwrap_or_default(),
                    ..StepTimes::default()
                });
            let seconds = step.duration_ms / 1000;
            if finished_at < midpoint {
                entry.before.push(seconds);
            } else {
                entry.after.push(seconds);
            }
        }
    }
    let mut slower: Vec<SlowerStep> = times
        .into_iter()
        .filter_map(|((pipeline_id, step), mut times)| {
            if times.before.len() < MIN_BUILDS_PER_HALF || times.after.len() < MIN_BUILDS_PER_HALF {
                return None;
            }
            let before = median(&mut times.before);
            let after = median(&mut times.after);
            let grew = after - before;
            #[allow(clippy::cast_precision_loss)]
            let enough = grew >= MIN_GROWTH_SECONDS && grew as f64 >= before as f64 * MIN_GROWTH;
            enough.then(|| SlowerStep {
                project_id: times.project_id,
                project_name: times.project_name,
                pipeline_id,
                pipeline_name: times.pipeline_name,
                step,
                before_seconds: before,
                after_seconds: after,
                builds: i64::try_from(times.before.len() + times.after.len()).unwrap_or(i64::MAX),
            })
        })
        .collect();
    slower.sort_by(|a, b| {
        (b.after_seconds - b.before_seconds).cmp(&(a.after_seconds - a.before_seconds))
    });
    slower.truncate(MAX_SLOWER);
    Ok(slower)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_760_000_000;

    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        for statement in [
            "INSERT INTO users (id, email, oidc_subject, role, created_at, updated_at) \
             VALUES ('u1', 'lead@example.com', 'sub', 'owner', 0, 0)",
            "INSERT INTO users (id, email, oidc_subject, role, created_at, updated_at) \
             VALUES ('u2', 'dev@example.com', 'sub2', 'developer', 0, 0)",
            "INSERT INTO projects (id, name, created_by, created_at, updated_at) \
             VALUES ('p1', 'Shop', 'u1', 0, 0)",
            "INSERT INTO pipelines (id, project_id, name, created_at, updated_at) \
             VALUES ('pl1', 'p1', 'android', 0, 0)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        pool
    }

    async fn build(pool: &SqlitePool, number: i64, sha: &str, status: &str, ago: i64, steps: &str) {
        sqlx::query(
            "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
             commit_sha, queued_at, finished_at, step_results, created_at, updated_at) \
             VALUES (?1, 'p1', 'pl1', ?2, ?3, 'manual', ?4, ?5, ?5, ?6, ?5, ?5)",
        )
        .bind(format!("b{number}"))
        .bind(number)
        .bind(status)
        .bind(sha)
        .bind(NOW - ago)
        .bind(steps)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO build_test_summaries (build_id, pipeline_id, created_at, updated_at) \
             VALUES (?1, 'pl1', 0, 0)",
        )
        .bind(format!("b{number}"))
        .execute(pool)
        .await
        .unwrap();
    }

    async fn failure(pool: &SqlitePool, number: i64, key: &str) {
        sqlx::query(
            "INSERT INTO build_test_failures (build_id, test_key, suite, name, step) \
             VALUES (?1, ?2, 'checkout_test.dart', ?2, 'Test')",
        )
        .bind(format!("b{number}"))
        .bind(key)
        .execute(pool)
        .await
        .unwrap();
    }

    fn steps(build_seconds: i64) -> String {
        serde_json::json!([{
            "name": "Build", "status": "succeeded", "started_at": 0, "finished_at": 0,
            "duration_ms": build_seconds * 1000
        }])
        .to_string()
    }

    #[tokio::test]
    async fn a_test_that_passes_on_the_same_commit_is_flaky() {
        let pool = pool().await;
        build(&pool, 1, "abc", "failed", 3 * DAY, "[]").await;
        failure(&pool, 1, "pays with card").await;
        failure(&pool, 1, "real failure").await;
        build(&pool, 2, "abc", "failed", 3 * DAY - 600, "[]").await;
        failure(&pool, 2, "real failure").await;
        // Another commit fixing a real failure is not flakiness.
        build(&pool, 3, "def", "succeeded", DAY, "[]").await;
        let trends = load_build_trends(&pool, None, 30, NOW).await.unwrap();
        assert_eq!(trends.flaky_tests.len(), 1);
        let flaky = &trends.flaky_tests[0];
        assert_eq!(flaky.name, "pays with card");
        assert_eq!(
            (flaky.failed_build_number, flaky.passed_build_number),
            (1, 2)
        );
        // A member of no app sees nothing.
        let none = load_build_trends(&pool, Some("u2"), 30, NOW).await.unwrap();
        assert!(none.flaky_tests.is_empty());
    }

    #[tokio::test]
    async fn a_step_that_got_slower_is_listed() {
        let pool = pool().await;
        for n in 0..4 {
            build(
                &pool,
                10 + n,
                &format!("old{n}"),
                "succeeded",
                25 * DAY + n * 60,
                &steps(300),
            )
            .await;
            build(
                &pool,
                20 + n,
                &format!("new{n}"),
                "succeeded",
                2 * DAY + n * 60,
                &steps(480),
            )
            .await;
        }
        let trends = load_build_trends(&pool, None, 30, NOW).await.unwrap();
        assert_eq!(trends.slower_steps.len(), 1);
        let step = &trends.slower_steps[0];
        assert_eq!(
            (step.before_seconds, step.after_seconds, step.builds),
            (300, 480, 8)
        );
    }
}
