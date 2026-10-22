//! Build health for the Home dashboard: outcomes, run time, queue wait and
//! trends over a window of days.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use oore_contract::{
    ApiError, BuildInsightsDay, BuildInsightsResponse, BuildOutcomeTotals, BuildQueueSnapshot,
    DurationPercentiles, FailingPipelineInsights, ProjectBuildInsights,
};
use serde::Deserialize;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use tracing::error;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

const DAY: i64 = 86_400;
const WINDOWS: [i64; 4] = [7, 14, 30, 90];
const MAX_PROJECTS: usize = 50;
const MAX_FAILING_PIPELINES: usize = 5;

#[derive(Debug, Deserialize)]
pub struct BuildInsightsQuery {
    pub days: Option<i64>,
    pub project_id: Option<String>,
    pub utc_offset_minutes: Option<i64>,
}

/// Who may see which builds, and which project to narrow to.
struct Scope<'a> {
    member_user_id: Option<&'a str>,
    project_id: Option<&'a str>,
}

impl Scope<'_> {
    fn push_conditions(&self, query: &mut QueryBuilder<Sqlite>) {
        if let Some(user_id) = self.member_user_id {
            query.push(
                " AND EXISTS (SELECT 1 FROM project_members pm \
                 WHERE pm.project_id = builds.project_id AND pm.user_id = ",
            );
            query.push_bind(user_id.to_string());
            query.push(")");
        }
        if let Some(project_id) = self.project_id {
            // `+` keeps the time range, not a project's whole history, as the
            // index SQLite searches.
            query.push(" AND +builds.project_id = ");
            query.push_bind(project_id.to_string());
        }
    }
}

struct FinishedBuild {
    id: String,
    status: String,
    project_id: String,
    project_name: String,
    pipeline_id: String,
    pipeline_name: String,
    build_number: i64,
    run_seconds: Option<i64>,
    finished_at: i64,
}

impl FinishedBuild {
    fn is_failure(&self) -> bool {
        matches!(self.status.as_str(), "failed" | "timed_out")
    }

    fn has_verdict(&self) -> bool {
        self.is_failure() || self.status == "succeeded"
    }

    /// Run time counts only succeeded builds, so a crash a minute in does not
    /// drag the median down.
    fn success_seconds(&self) -> Option<i64> {
        (self.status == "succeeded")
            .then_some(self.run_seconds)
            .flatten()
    }
}

/// `GET /v1/builds/insights`
pub async fn get_build_insights(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<BuildInsightsQuery>,
) -> ApiResult<BuildInsightsResponse> {
    let days = params.days.unwrap_or(7);
    if !WINDOWS.contains(&days) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "days must be 7, 14, 30 or 90",
        ));
    }
    let offset_minutes = params.utc_offset_minutes.unwrap_or(0);
    if !(-720..=840).contains(&offset_minutes) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "utc_offset_minutes must be between -720 and 840",
        ));
    }
    let privileged = auth.0.role == "owner" || auth.0.role == "admin";
    let scope = Scope {
        member_user_id: (!privileged).then_some(auth.0.user_id.as_str()),
        project_id: params.project_id.as_deref(),
    };
    load_build_insights(&state.db, &scope, days, offset_minutes, now_unix())
        .await
        .map(Json)
        .map_err(|e| {
            error!(error = %e, "failed to load build insights");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load build insights",
            )
        })
}

/// Unix seconds at the start of the caller's local day containing `now`.
fn local_day_start(now: i64, offset_minutes: i64) -> i64 {
    let offset = offset_minutes * 60;
    let local = now + offset;
    local - local.rem_euclid(DAY) - offset
}

async fn load_build_insights(
    pool: &SqlitePool,
    scope: &Scope<'_>,
    days: i64,
    offset_minutes: i64,
    now: i64,
) -> Result<BuildInsightsResponse, sqlx::Error> {
    let since = local_day_start(now, offset_minutes) - (days - 1) * DAY;
    let previous_since = since - days * DAY;

    let finished: Vec<FinishedBuild> = finished_query(scope, previous_since)
        .build()
        .fetch_all(pool)
        .await?
        .iter()
        .map(|row| {
            let started_at: Option<i64> = row.get("started_at");
            let finished_at: i64 = row.get("finished_at");
            let project_id: String = row.get("project_id");
            let pipeline_id: String = row.get("pipeline_id");
            FinishedBuild {
                id: row.get("id"),
                status: row.get("status"),
                project_name: row
                    .get::<Option<String>, _>("project_name")
                    .unwrap_or_else(|| project_id.clone()),
                pipeline_name: row
                    .get::<Option<String>, _>("pipeline_name")
                    .unwrap_or_else(|| pipeline_id.clone()),
                project_id,
                pipeline_id,
                build_number: row.get("build_number"),
                run_seconds: started_at.map(|start| (finished_at - start).max(0)),
                finished_at,
            }
        })
        .collect();
    let (previous, current): (Vec<&FinishedBuild>, Vec<&FinishedBuild>) =
        finished.iter().partition(|build| build.finished_at < since);

    let waits: Vec<i64> = wait_query(scope, since)
        .build()
        .fetch_all(pool)
        .await?
        .iter()
        .map(|row| row.get::<i64, _>("wait").max(0))
        .collect();

    let mut query: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT builds.status, builds.approval_state, builds.queued_at FROM builds \
         WHERE builds.status IN ('queued', 'scheduled', 'assigned', 'running')",
    );
    scope.push_conditions(&mut query);
    let mut queue = BuildQueueSnapshot::default();
    for row in query.build().fetch_all(pool).await? {
        let status: String = row.get("status");
        let approval_state: Option<String> = row.get("approval_state");
        let queued_at: i64 = row.get("queued_at");
        match status.as_str() {
            "assigned" | "running" => queue.running += 1,
            _ if approval_state.as_deref() == Some("pending") => queue.awaiting_approval += 1,
            _ => {
                queue.waiting += 1;
                queue.oldest_waiting_since = Some(
                    queue
                        .oldest_waiting_since
                        .map_or(queued_at, |oldest| oldest.min(queued_at)),
                );
            }
        }
    }

    Ok(BuildInsightsResponse {
        window_days: days,
        since,
        totals: totals(&current),
        previous_totals: totals(&previous),
        duration: percentiles(current.iter().filter_map(|b| b.success_seconds()).collect()),
        previous_duration: percentiles(
            previous
                .iter()
                .filter_map(|b| b.success_seconds())
                .collect(),
        ),
        queue_wait: percentiles(waits),
        daily: daily(&current, since, days),
        queue,
        projects: projects(&current),
        failing_pipelines: failing_pipelines(&current),
    })
}

/// Builds that finished in the current or previous window.
fn finished_query(scope: &Scope<'_>, previous_since: i64) -> QueryBuilder<Sqlite> {
    let mut query = QueryBuilder::new(
        "SELECT builds.id, builds.status, builds.project_id, builds.pipeline_id, builds.build_number, \
         builds.started_at, builds.finished_at, projects.name AS project_name, pipelines.name AS pipeline_name \
         FROM builds \
         LEFT JOIN projects ON projects.id = builds.project_id \
         LEFT JOIN pipelines ON pipelines.id = builds.pipeline_id \
         WHERE builds.finished_at >= ",
    );
    query.push_bind(previous_since);
    // The unary `+` keeps SQLite from using the status index: nearly every
    // build is terminal, so the finished_at range is what narrows the scan.
    query
        .push(" AND +builds.status IN ('succeeded', 'failed', 'timed_out', 'canceled', 'expired')");
    scope.push_conditions(&mut query);
    query.push(" ORDER BY builds.finished_at ASC, builds.build_number ASC");
    query
}

/// How long builds that started in the window waited for a runner.
fn wait_query(scope: &Scope<'_>, since: i64) -> QueryBuilder<Sqlite> {
    // Approval time is a maintainer's wait, not the queue's.
    let mut query = QueryBuilder::new(
        "SELECT builds.started_at - COALESCE(builds.approval_decided_at, builds.queued_at) AS wait \
         FROM builds WHERE builds.started_at >= ",
    );
    query.push_bind(since);
    scope.push_conditions(&mut query);
    query
}

fn totals(builds: &[&FinishedBuild]) -> BuildOutcomeTotals {
    let mut totals = BuildOutcomeTotals {
        finished: builds.len() as i64,
        ..BuildOutcomeTotals::default()
    };
    for build in builds {
        match build.status.as_str() {
            "succeeded" => totals.succeeded += 1,
            "failed" => totals.failed += 1,
            "timed_out" => totals.timed_out += 1,
            "canceled" => totals.canceled += 1,
            _ => totals.expired += 1,
        }
    }
    let verdicts = totals.succeeded + totals.failed + totals.timed_out;
    totals.success_rate = (verdicts > 0).then(|| totals.succeeded as f64 / verdicts as f64);
    totals
}

/// Nearest-rank percentiles.
fn percentiles(mut seconds: Vec<i64>) -> DurationPercentiles {
    seconds.sort_unstable();
    let rank = |p: f64| {
        let index = ((p * seconds.len() as f64).ceil() as usize).max(1) - 1;
        seconds.get(index).copied()
    };
    DurationPercentiles {
        p50_seconds: rank(0.5),
        p90_seconds: rank(0.9),
        samples: seconds.len() as i64,
    }
}

fn daily(builds: &[&FinishedBuild], since: i64, days: i64) -> Vec<BuildInsightsDay> {
    let mut buckets: Vec<(BuildInsightsDay, Vec<i64>)> = (0..days)
        .map(|day| {
            let bucket = BuildInsightsDay {
                day_start: since + day * DAY,
                succeeded: 0,
                failed: 0,
                other: 0,
                p50_duration_seconds: None,
            };
            (bucket, Vec::new())
        })
        .collect();
    for build in builds {
        let index = ((build.finished_at - since) / DAY) as usize;
        let Some((bucket, durations)) = buckets.get_mut(index) else {
            continue;
        };
        if build.status == "succeeded" {
            bucket.succeeded += 1;
        } else if build.is_failure() {
            bucket.failed += 1;
        } else {
            bucket.other += 1;
        }
        durations.extend(build.success_seconds());
    }
    buckets
        .into_iter()
        .map(|(mut bucket, durations)| {
            bucket.p50_duration_seconds = percentiles(durations).p50_seconds;
            bucket
        })
        .collect()
}

fn projects(builds: &[&FinishedBuild]) -> Vec<ProjectBuildInsights> {
    let mut by_project: HashMap<&str, Vec<&FinishedBuild>> = HashMap::new();
    for build in builds {
        by_project.entry(&build.project_id).or_default().push(build);
    }
    let mut projects: Vec<ProjectBuildInsights> = by_project
        .into_values()
        .map(|builds| {
            // Builds arrive oldest first.
            let last = builds[builds.len() - 1];
            ProjectBuildInsights {
                project_id: last.project_id.clone(),
                project_name: last.project_name.clone(),
                totals: totals(&builds),
                duration: percentiles(builds.iter().filter_map(|b| b.success_seconds()).collect()),
                last_status: last.status.clone(),
                last_finished_at: last.finished_at,
            }
        })
        .collect();
    projects.sort_by(|a, b| {
        b.totals
            .finished
            .cmp(&a.totals.finished)
            .then_with(|| a.project_name.cmp(&b.project_name))
    });
    projects.truncate(MAX_PROJECTS);
    projects
}

fn failing_pipelines(builds: &[&FinishedBuild]) -> Vec<FailingPipelineInsights> {
    let mut by_pipeline: HashMap<&str, Vec<&FinishedBuild>> = HashMap::new();
    for build in builds {
        by_pipeline
            .entry(&build.pipeline_id)
            .or_default()
            .push(build);
    }
    let mut pipelines: Vec<FailingPipelineInsights> = by_pipeline
        .into_values()
        .filter_map(|builds| {
            let last_failed = builds.iter().rev().find(|b| b.is_failure())?;
            let current_streak = builds
                .iter()
                .rev()
                .filter(|b| b.has_verdict())
                .take_while(|b| b.is_failure())
                .count() as i64;
            Some(FailingPipelineInsights {
                pipeline_id: last_failed.pipeline_id.clone(),
                pipeline_name: last_failed.pipeline_name.clone(),
                project_id: last_failed.project_id.clone(),
                project_name: last_failed.project_name.clone(),
                failures: builds.iter().filter(|b| b.is_failure()).count() as i64,
                finished: builds.len() as i64,
                current_streak,
                last_failed_build_id: last_failed.id.clone(),
                last_failed_build_number: last_failed.build_number,
                last_failed_at: last_failed.finished_at,
            })
        })
        .collect();
    pipelines.sort_by(|a, b| {
        b.current_streak
            .cmp(&a.current_streak)
            .then_with(|| b.failures.cmp(&a.failures))
            .then_with(|| b.last_failed_at.cmp(&a.last_failed_at))
    });
    pipelines.truncate(MAX_FAILING_PIPELINES);
    pipelines
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
            "INSERT INTO projects (id, name, created_by, created_at, updated_at) \
             VALUES ('p2', 'Payments', 'u1', 0, 0)",
            "INSERT INTO pipelines (id, project_id, name, created_at, updated_at) \
             VALUES ('pl1', 'p1', 'android', 0, 0)",
            "INSERT INTO pipelines (id, project_id, name, created_at, updated_at) \
             VALUES ('pl2', 'p2', 'ios', 0, 0)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        pool
    }

    async fn build(
        pool: &SqlitePool,
        pipeline: (&str, &str),
        number: i64,
        status: &str,
        queued_ago: i64,
        started_ago: Option<i64>,
        finished_ago: Option<i64>,
    ) {
        sqlx::query(
            "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
             queued_at, started_at, finished_at, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'manual', ?6, ?7, ?8, ?6, ?6)",
        )
        .bind(format!("{}-{number}", pipeline.1))
        .bind(pipeline.0)
        .bind(pipeline.1)
        .bind(number)
        .bind(status)
        .bind(NOW - queued_ago)
        .bind(started_ago.map(|ago| NOW - ago))
        .bind(finished_ago.map(|ago| NOW - ago))
        .execute(pool)
        .await
        .unwrap();
    }

    async fn finished(
        pool: &SqlitePool,
        pipeline: (&str, &str),
        number: i64,
        status: &str,
        ago: i64,
        run: i64,
    ) {
        build(
            pool,
            pipeline,
            number,
            status,
            ago + run + 60,
            Some(ago + run),
            Some(ago),
        )
        .await;
    }

    const SHOP: (&str, &str) = ("p1", "pl1");
    const PAYMENTS: (&str, &str) = ("p2", "pl2");
    const OWNER: Scope<'static> = Scope {
        member_user_id: None,
        project_id: None,
    };

    #[test]
    fn day_start_follows_the_callers_offset() {
        // 2025-10-09T08:53:20Z; in IST (+05:30) the day began at 2025-10-08T18:30Z.
        assert_eq!(local_day_start(NOW, 0), 1_759_968_000);
        assert_eq!(local_day_start(NOW, 330), 1_759_948_200);
        assert_eq!(local_day_start(NOW, -420), 1_759_993_200);
    }

    #[test]
    fn percentiles_use_nearest_rank() {
        let p = percentiles(vec![50, 10, 40, 20, 30]);
        assert_eq!(
            (p.p50_seconds, p.p90_seconds, p.samples),
            (Some(30), Some(50), 5)
        );
        assert_eq!(percentiles(Vec::new()).p50_seconds, None);
    }

    #[tokio::test]
    async fn summarises_outcomes_against_the_previous_window() {
        let pool = pool().await;
        finished(&pool, SHOP, 1, "succeeded", 3_600, 600).await;
        finished(&pool, SHOP, 2, "failed", 7_200, 120).await;
        finished(&pool, SHOP, 3, "succeeded", 2 * DAY, 300).await;
        finished(&pool, SHOP, 4, "canceled", 3 * DAY, 30).await;
        // Previous window.
        finished(&pool, SHOP, 5, "succeeded", 9 * DAY, 900).await;
        // Too old for either window.
        finished(&pool, SHOP, 6, "failed", 20 * DAY, 60).await;

        let insights = load_build_insights(&pool, &OWNER, 7, 0, NOW).await.unwrap();

        assert_eq!(insights.totals.finished, 4);
        assert_eq!(insights.totals.succeeded, 2);
        assert_eq!(insights.totals.failed, 1);
        assert_eq!(insights.totals.canceled, 1);
        assert_eq!(insights.totals.success_rate, Some(2.0 / 3.0));
        assert_eq!(insights.previous_totals.finished, 1);
        assert_eq!(insights.duration.p50_seconds, Some(300));
        assert_eq!(insights.duration.p90_seconds, Some(600));
        assert_eq!(insights.previous_duration.p50_seconds, Some(900));
        assert_eq!(insights.queue_wait.p50_seconds, Some(60));
        assert_eq!(insights.daily.len(), 7);
        let today = &insights.daily[6];
        assert_eq!((today.succeeded, today.failed, today.other), (1, 1, 0));
        assert_eq!(today.p50_duration_seconds, Some(600));
    }

    #[tokio::test]
    async fn ranks_failing_pipelines_by_current_streak() {
        let pool = pool().await;
        finished(&pool, SHOP, 1, "failed", 5 * DAY, 60).await;
        finished(&pool, SHOP, 2, "failed", 4 * DAY, 60).await;
        finished(&pool, SHOP, 3, "succeeded", 3 * DAY, 60).await;
        finished(&pool, PAYMENTS, 1, "succeeded", 3 * DAY, 60).await;
        finished(&pool, PAYMENTS, 2, "failed", 2 * DAY, 60).await;
        finished(&pool, PAYMENTS, 3, "canceled", DAY + 100, 60).await;
        finished(&pool, PAYMENTS, 4, "timed_out", DAY, 60).await;

        let insights = load_build_insights(&pool, &OWNER, 7, 0, NOW).await.unwrap();

        let failing: Vec<_> = insights
            .failing_pipelines
            .iter()
            .map(|p| {
                (
                    p.pipeline_name.as_str(),
                    p.failures,
                    p.current_streak,
                    p.last_failed_build_number,
                )
            })
            .collect();
        assert_eq!(failing, [("ios", 2, 2, 4), ("android", 2, 0, 2)]);

        let projects: Vec<_> = insights
            .projects
            .iter()
            .map(|p| {
                (
                    p.project_name.as_str(),
                    p.totals.finished,
                    p.last_status.as_str(),
                )
            })
            .collect();
        assert_eq!(
            projects,
            [("Payments", 4, "timed_out"), ("Shop", 3, "succeeded")]
        );
    }

    #[tokio::test]
    async fn counts_the_live_queue_without_approval_holds() {
        let pool = pool().await;
        build(&pool, SHOP, 1, "queued", 900, None, None).await;
        build(&pool, SHOP, 2, "queued", 300, None, None).await;
        build(&pool, SHOP, 3, "running", 600, Some(500), None).await;
        build(&pool, PAYMENTS, 1, "queued", 1_200, None, None).await;
        sqlx::query("UPDATE builds SET approval_state = 'pending' WHERE id = 'pl2-1'")
            .execute(&pool)
            .await
            .unwrap();

        let insights = load_build_insights(&pool, &OWNER, 7, 0, NOW).await.unwrap();

        assert_eq!(insights.queue.waiting, 2);
        assert_eq!(insights.queue.awaiting_approval, 1);
        assert_eq!(insights.queue.running, 1);
        assert_eq!(insights.queue.oldest_waiting_since, Some(NOW - 900));
        assert_eq!(insights.queue_wait.p50_seconds, Some(100));
    }

    #[tokio::test]
    async fn members_only_see_their_projects() {
        let pool = pool().await;
        finished(&pool, SHOP, 1, "succeeded", 3_600, 60).await;
        finished(&pool, PAYMENTS, 1, "failed", 3_600, 60).await;
        sqlx::query(
            "INSERT INTO project_members (id, project_id, user_id, role, created_by, created_at, updated_at) \
             VALUES ('m1', 'p1', 'u2', 'developer', 'u1', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let member = Scope {
            member_user_id: Some("u2"),
            project_id: None,
        };

        let insights = load_build_insights(&pool, &member, 7, 0, NOW)
            .await
            .unwrap();

        assert_eq!(insights.totals.finished, 1);
        assert_eq!(insights.totals.succeeded, 1);
        assert!(insights.failing_pipelines.is_empty());
    }

    async fn query_plan(pool: &SqlitePool, query: QueryBuilder<Sqlite>) -> String {
        let sql = format!("EXPLAIN QUERY PLAN {}", query.sql().as_str());
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .fetch_all(pool)
            .await
            .unwrap()
            .iter()
            .map(|row| row.get::<String, _>("detail"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[tokio::test]
    async fn window_queries_search_by_time_instead_of_scanning_history() {
        let pool = pool().await;

        let project = Scope {
            member_user_id: Some("u2"),
            project_id: Some("p1"),
        };
        for scope in [&OWNER, &project] {
            let finished = query_plan(&pool, finished_query(scope, NOW)).await;
            assert!(
                finished
                    .contains("SEARCH builds USING INDEX idx_builds_finished_at (finished_at>?)"),
                "{finished}"
            );
        }
        let waits = query_plan(&pool, wait_query(&OWNER, NOW)).await;
        assert!(
            waits.contains("SEARCH builds USING INDEX idx_builds_started_at (started_at>?)"),
            "{waits}"
        );
    }
}
