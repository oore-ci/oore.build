//! `dart analyze` summaries reported by runners, and how they compare with
//! the baseline build on the base branch.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use oore_contract::{
    AnalyzerCounts, AnalyzerDiagnostic, ApiError, BuildAnalysis, BuildAnalysisBaseline,
    BuildAnalysisResponse, OkResponse, RunnerAnalyzerReport,
};
use sqlx::{Row, SqlitePool};
use tracing::error;

use crate::AppState;
use crate::artifact_sizes::{BaselineData, find_baseline};
use crate::extractors::AuthUser;
use crate::logs::require_build_log_read;
use crate::runners::RunnerAuth;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

/// Diagnostics kept per build; the counts include every diagnostic.
const MAX_DIAGNOSTICS: usize = 50;
const MAX_MESSAGE_BYTES: usize = 1024;
const MAX_FILE_BYTES: usize = 512;
const MAX_CODE_BYTES: usize = 128;

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

/// `POST /v1/runners/{runner_id}/jobs/{job_id}/analysis`
pub async fn report_job_analysis(
    State(state): State<Arc<AppState>>,
    Path((runner_id, job_id)): Path<(String, String)>,
    runner_auth: RunnerAuth,
    Json(report): Json<RunnerAnalyzerReport>,
) -> ApiResult<OkResponse> {
    if runner_auth.runner_id != runner_id {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "runner_mismatch",
            "Runner token does not match the requested runner ID",
        ));
    }
    let row = sqlx::query("SELECT runner_id, status FROM builds WHERE id = ?1")
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
    if row.get::<String, _>("status") != "running" {
        return Err(api_err(
            StatusCode::CONFLICT,
            "job_not_active",
            "An analysis can only be reported while the build is running",
        ));
    }
    record_analysis(&state.db, &job_id, &report)
        .await
        .map_err(|e| store_error("Failed to record the analysis", e))?;
    Ok(Json(OkResponse { ok: true }))
}

/// Store a build's analysis, replacing an earlier one.
pub(crate) async fn record_analysis(
    pool: &SqlitePool,
    build_id: &str,
    report: &RunnerAnalyzerReport,
) -> Result<(), sqlx::Error> {
    let diagnostics: Vec<AnalyzerDiagnostic> = report
        .diagnostics
        .iter()
        .take(MAX_DIAGNOSTICS)
        .map(|diagnostic| AnalyzerDiagnostic {
            severity: diagnostic.severity,
            code: truncate(&diagnostic.code, MAX_CODE_BYTES),
            message: truncate(&diagnostic.message, MAX_MESSAGE_BYTES),
            file: truncate(&diagnostic.file, MAX_FILE_BYTES),
            line: diagnostic.line.max(0),
            column: diagnostic.column.max(0),
        })
        .collect();
    sqlx::query(
        "INSERT INTO build_analyses (build_id, errors, warnings, infos, diagnostics, recorded_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT(build_id) DO UPDATE SET errors = excluded.errors, \
         warnings = excluded.warnings, infos = excluded.infos, \
         diagnostics = excluded.diagnostics, recorded_at = excluded.recorded_at",
    )
    .bind(build_id)
    .bind(report.counts.errors.max(0))
    .bind(report.counts.warnings.max(0))
    .bind(report.counts.infos.max(0))
    .bind(serde_json::to_string(&diagnostics).unwrap_or_else(|_| "[]".to_string()))
    .bind(now_unix())
    .execute(pool)
    .await?;
    Ok(())
}

/// `GET /v1/builds/{build_id}/analysis`
pub async fn get_build_analysis(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
) -> ApiResult<BuildAnalysisResponse> {
    require_build_log_read(&state.db, &auth.0, &build_id).await?;
    load_analysis_report(&state.db, &build_id)
        .await
        .map_err(|e| store_error("Failed to load the analysis", e))?
        .map(Json)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Build not found"))
}

async fn load_analysis(
    pool: &SqlitePool,
    build_id: &str,
) -> Result<Option<BuildAnalysis>, sqlx::Error> {
    Ok(sqlx::query(
        "SELECT errors, warnings, infos, diagnostics, recorded_at FROM build_analyses \
         WHERE build_id = ?1",
    )
    .bind(build_id)
    .fetch_optional(pool)
    .await?
    .map(|row| BuildAnalysis {
        counts: AnalyzerCounts {
            errors: row.get("errors"),
            warnings: row.get("warnings"),
            infos: row.get("infos"),
        },
        diagnostics: serde_json::from_str(&row.get::<String, _>("diagnostics")).unwrap_or_default(),
        recorded_at: row.get("recorded_at"),
    }))
}

/// A build's analysis and the baseline it compares with. `None` when the
/// build does not exist. A build without an analysis has no baseline either.
pub(crate) async fn load_analysis_report(
    pool: &SqlitePool,
    build_id: &str,
) -> Result<Option<BuildAnalysisResponse>, sqlx::Error> {
    let Some((base_branch, baseline)) =
        find_baseline(pool, build_id, BaselineData::Analysis).await?
    else {
        return Ok(None);
    };
    let analysis = load_analysis(pool, build_id).await?;
    let baseline = match (&analysis, baseline) {
        (Some(_), Some(baseline)) => {
            load_analysis(pool, &baseline.build_id)
                .await?
                .map(|base| BuildAnalysisBaseline {
                    build_id: baseline.build_id,
                    build_number: baseline.build_number,
                    branch: baseline.branch,
                    counts: base.counts,
                })
        }
        _ => None,
    };
    Ok(Some(BuildAnalysisResponse {
        analysis,
        base_branch,
        baseline,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use oore_contract::AnalyzerSeverity;

    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        for statement in [
            "INSERT INTO users (id, email, oidc_subject, created_at, updated_at) \
             VALUES ('u1', 'dev@example.com', 'sub', 0, 0)",
            "INSERT INTO projects (id, name, default_branch, created_by, created_at, updated_at) \
             VALUES ('p1', 'App', 'main', 'u1', 0, 0)",
            "INSERT INTO pipelines (id, project_id, name, created_at, updated_at) \
             VALUES ('pl1', 'p1', 'Android', 0, 0)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        pool
    }

    async fn insert_build(pool: &SqlitePool, id: &str, number: i64, branch: &str, status: &str) {
        sqlx::query(
            "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, branch, commit_sha, config_snapshot, queued_at, created_at, updated_at) \
             VALUES (?1, 'p1', 'pl1', ?2, ?3, 'manual', ?4, ?1, '{}', ?2, ?2, ?2)",
        )
        .bind(id)
        .bind(number)
        .bind(status)
        .bind(branch)
        .execute(pool)
        .await
        .unwrap();
    }

    fn report(errors: i64, warnings: i64, infos: i64) -> RunnerAnalyzerReport {
        RunnerAnalyzerReport {
            counts: AnalyzerCounts {
                errors,
                warnings,
                infos,
            },
            diagnostics: vec![AnalyzerDiagnostic {
                severity: AnalyzerSeverity::Warning,
                code: "unused_import".to_string(),
                message: "Unused import: 'dart:async'.".to_string(),
                file: "lib/main.dart".to_string(),
                line: 3,
                column: 8,
            }],
        }
    }

    #[tokio::test]
    async fn compares_with_the_latest_analyzed_build_on_the_base_branch() {
        let pool = pool().await;
        insert_build(&pool, "b1", 1, "main", "succeeded").await;
        insert_build(&pool, "b2", 2, "main", "failed").await;
        insert_build(&pool, "b3", 3, "main", "succeeded").await;
        insert_build(&pool, "b4", 4, "feature", "succeeded").await;
        record_analysis(&pool, "b1", &report(0, 1, 4))
            .await
            .unwrap();
        record_analysis(&pool, "b2", &report(3, 3, 3))
            .await
            .unwrap();
        // b3 has no analysis, b2 failed: b1 is the baseline.
        record_analysis(&pool, "b4", &report(0, 3, 4))
            .await
            .unwrap();

        let response = load_analysis_report(&pool, "b4").await.unwrap().unwrap();
        assert_eq!(response.base_branch.as_deref(), Some("main"));
        let baseline = response.baseline.unwrap();
        assert_eq!(baseline.build_id, "b1");
        assert_eq!(baseline.counts.warnings, 1);
        let analysis = response.analysis.unwrap();
        assert_eq!(analysis.counts.warnings, 3);
        assert_eq!(analysis.diagnostics[0].code, "unused_import");

        // A later report replaces the earlier one.
        record_analysis(&pool, "b4", &report(0, 2, 0))
            .await
            .unwrap();
        let response = load_analysis_report(&pool, "b4").await.unwrap().unwrap();
        assert_eq!(response.analysis.unwrap().counts.warnings, 2);
    }

    #[tokio::test]
    async fn a_build_without_an_analysis_has_none_and_no_baseline() {
        let pool = pool().await;
        insert_build(&pool, "b1", 1, "main", "succeeded").await;
        insert_build(&pool, "b2", 2, "feature", "succeeded").await;
        record_analysis(&pool, "b1", &report(0, 1, 0))
            .await
            .unwrap();
        let response = load_analysis_report(&pool, "b2").await.unwrap().unwrap();
        assert_eq!(response.analysis, None);
        assert_eq!(response.baseline, None);
        assert!(
            load_analysis_report(&pool, "missing")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn stored_diagnostics_are_capped_and_trimmed() {
        let pool = pool().await;
        insert_build(&pool, "b1", 1, "main", "succeeded").await;
        let mut big = report(0, 80, 0);
        big.diagnostics = (0..80)
            .map(|index| AnalyzerDiagnostic {
                severity: AnalyzerSeverity::Warning,
                code: "dead_code".to_string(),
                message: "x".repeat(5000),
                file: format!("lib/f{index}.dart"),
                line: -1,
                column: 2,
            })
            .collect();
        record_analysis(&pool, "b1", &big).await.unwrap();
        let analysis = load_analysis(&pool, "b1").await.unwrap().unwrap();
        assert_eq!(analysis.counts.warnings, 80);
        assert_eq!(analysis.diagnostics.len(), MAX_DIAGNOSTICS);
        assert_eq!(analysis.diagnostics[0].message.len(), MAX_MESSAGE_BYTES);
        assert_eq!(analysis.diagnostics[0].line, 0);
    }
}
