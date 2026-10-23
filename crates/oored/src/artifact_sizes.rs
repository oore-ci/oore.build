//! Artifact sizes per build and their change from a baseline build: the latest
//! earlier successful build of the same pipeline on the base branch.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use oore_contract::{ApiError, ArtifactSizeComparison, BuildSizeBaseline, BuildSizeReport};
use sqlx::{Row, SqlitePool};
use tracing::{error, warn};

use crate::AppState;
use crate::artifacts::require_project_artifact_read;
use crate::extractors::AuthUser;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

/// Remember an artifact's size once its upload is finished. Best effort: a
/// missing size only leaves a gap in the comparison.
pub(crate) async fn record_artifact_size(pool: &SqlitePool, artifact_id: &str) {
    let result = sqlx::query(
        "INSERT OR IGNORE INTO build_artifact_sizes \
             (artifact_id, build_id, name, artifact_type, size_bytes, recorded_at) \
         SELECT id, build_id, name, artifact_type, file_size, ?2 FROM artifacts \
         WHERE id = ?1 AND state = 'available' AND file_size IS NOT NULL AND file_size >= 0",
    )
    .bind(artifact_id)
    .bind(now_unix())
    .execute(pool)
    .await;
    if let Err(error) = result {
        warn!(%error, artifact_id, "failed to record artifact size");
    }
}

/// `GET /v1/builds/{build_id}/sizes`
pub async fn get_build_size_report(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
) -> ApiResult<BuildSizeReport> {
    let store_error = |e: sqlx::Error| {
        error!(error = %e, "failed to load artifact sizes");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load artifact sizes",
        )
    };
    let project_id: String = sqlx::query_scalar("SELECT project_id FROM builds WHERE id = ?1")
        .bind(&build_id)
        .fetch_optional(&state.db)
        .await
        .map_err(store_error)?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Build not found"))?;
    require_project_artifact_read(&state.db, &auth, &project_id).await?;
    load_size_report(&state.db, &build_id)
        .await
        .map_err(store_error)?
        .map(Json)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Build not found"))
}

#[derive(Debug, Clone)]
struct SizeRow {
    artifact_id: String,
    name: String,
    artifact_type: String,
    size_bytes: i64,
}

async fn load_sizes(pool: &SqlitePool, build_id: &str) -> Result<Vec<SizeRow>, sqlx::Error> {
    Ok(sqlx::query(
        "SELECT artifact_id, name, artifact_type, size_bytes FROM build_artifact_sizes \
         WHERE build_id = ?1 ORDER BY name, artifact_id",
    )
    .bind(build_id)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|row| SizeRow {
        artifact_id: row.get("artifact_id"),
        name: row.get("name"),
        artifact_type: row.get("artifact_type"),
        size_bytes: row.get("size_bytes"),
    })
    .collect())
}

/// The branch a pull request targets, from the webhook that started it.
fn pull_request_target(payload: &serde_json::Value) -> Option<String> {
    payload
        .pointer("/pull_request/base/ref")
        .or_else(|| payload.pointer("/object_attributes/target_branch"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|branch| !branch.is_empty())
        .map(str::to_string)
}

/// Reruns chain back to the build a trigger started.
const MAX_RERUN_DEPTH: i64 = 50;
/// Earlier successful builds on the base branch checked for a baseline.
const MAX_BASELINE_CANDIDATES: i64 = 25;

/// The target branch when the trigger that started a build, following reruns
/// back to it, was a pull or merge request. `Some(None)` is a pull request
/// whose target is unknown; `None` is not a pull request.
async fn pull_request_origin(
    pool: &SqlitePool,
    build_id: &str,
) -> Result<Option<Option<String>>, sqlx::Error> {
    // A rerun has its own trigger event and no webhook, so the origin is the
    // first build in its chain.
    let Some(origin) = sqlx::query(
        "WITH RECURSIVE chain(id, source_build_id, depth) AS ( \
             SELECT id, source_build_id, 0 FROM builds WHERE id = ?1 \
             UNION ALL \
             SELECT b.id, b.source_build_id, chain.depth + 1 FROM builds b \
             JOIN chain ON b.id = chain.source_build_id WHERE chain.depth < ?2 \
         ) \
         SELECT b.trigger_event, w.event_type, w.payload FROM chain \
         JOIN builds b ON b.id = chain.id \
         LEFT JOIN integration_webhooks w ON w.id = b.webhook_id \
         ORDER BY chain.depth DESC LIMIT 1",
    )
    .bind(build_id)
    .bind(MAX_RERUN_DEPTH)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    let is_pull_request = ["trigger_event", "event_type"].iter().any(|column| {
        origin
            .get::<Option<String>, _>(*column)
            .is_some_and(|event| {
                oore_contract::TriggerConfig::normalize_event(&event) == "pull_request"
            })
    });
    if !is_pull_request {
        return Ok(None);
    }
    Ok(Some(
        origin
            .get::<Option<String>, _>("payload")
            .and_then(|payload| serde_json::from_str::<serde_json::Value>(&payload).ok())
            .as_ref()
            .and_then(pull_request_target),
    ))
}

/// Sizes of a build compared with its baseline. `None` when the build does
/// not exist.
pub(crate) async fn load_size_report(
    pool: &SqlitePool,
    build_id: &str,
) -> Result<Option<BuildSizeReport>, sqlx::Error> {
    let Some(build) = sqlx::query(
        "SELECT b.pipeline_id, b.build_number, \
                p.default_branch AS project_default_branch, \
                r.default_branch AS repository_default_branch \
         FROM builds b \
         JOIN projects p ON p.id = b.project_id \
         LEFT JOIN integration_repositories r ON r.id = p.repository_id \
         WHERE b.id = ?1",
    )
    .bind(build_id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    let pull_request_base = pull_request_origin(pool, build_id).await?.flatten();
    let non_empty = |column: &str| {
        build
            .get::<Option<String>, _>(column)
            .map(|branch| branch.trim().to_string())
            .filter(|branch| !branch.is_empty())
    };
    let base_branch = pull_request_base
        .or_else(|| non_empty("project_default_branch"))
        .or_else(|| non_empty("repository_default_branch"));

    let current = load_sizes(pool, build_id).await?;

    let mut baseline = None;
    if let Some(branch) = &base_branch {
        let candidates = sqlx::query(
            "SELECT b.id, b.build_number, b.branch FROM builds b \
             WHERE b.pipeline_id = ?1 AND b.branch = ?2 AND b.build_number < ?3 \
               AND (b.status = 'succeeded' OR (b.status = 'expired' AND b.expired_from = 'succeeded')) \
               AND EXISTS (SELECT 1 FROM build_artifact_sizes s WHERE s.build_id = b.id) \
             ORDER BY b.build_number DESC LIMIT ?4",
        )
        .bind(build.get::<String, _>("pipeline_id"))
        .bind(branch)
        .bind(build.get::<i64, _>("build_number"))
        .bind(MAX_BASELINE_CANDIDATES)
        .fetch_all(pool)
        .await?;
        // A pull request whose head branch has the base branch's name, for
        // example a fork's `main`, is not the base branch.
        for row in candidates {
            let id: String = row.get("id");
            if pull_request_origin(pool, &id).await?.is_none() {
                baseline = Some(BuildSizeBaseline {
                    build_id: id,
                    build_number: row.get("build_number"),
                    branch: row.get("branch"),
                });
                break;
            }
        }
    }
    let baseline_sizes = match &baseline {
        Some(baseline) => load_sizes(pool, &baseline.build_id).await?,
        None => Vec::new(),
    };

    Ok(Some(BuildSizeReport {
        base_branch,
        baseline,
        artifacts: compare_sizes(&current, &baseline_sizes),
    }))
}

/// The kind of file, so `app-1.2.0.apk` can match `app-1.3.0.apk`.
fn file_kind(row: &SizeRow) -> String {
    match row.name.rsplit_once('.') {
        Some((_, extension)) if !extension.is_empty() => extension.to_ascii_lowercase(),
        _ => row.artifact_type.clone(),
    }
}

/// Pair each artifact with a baseline artifact: the same name first, then by
/// kind when each build has exactly one artifact of that kind. Split APKs of
/// different ABIs therefore only pair up by name.
fn compare_sizes(current: &[SizeRow], baseline: &[SizeRow]) -> Vec<ArtifactSizeComparison> {
    let mut used = vec![false; baseline.len()];
    let mut matched: Vec<Option<usize>> = current
        .iter()
        .map(|artifact| {
            let index = baseline
                .iter()
                .enumerate()
                .position(|(index, base)| !used[index] && base.name == artifact.name)?;
            used[index] = true;
            Some(index)
        })
        .collect();

    for (position, artifact) in current.iter().enumerate() {
        if matched[position].is_some() {
            continue;
        }
        let kind = file_kind(artifact);
        let same_kind_current = current.iter().filter(|other| file_kind(other) == kind);
        let same_kind_baseline: Vec<usize> = baseline
            .iter()
            .enumerate()
            .filter(|(_, base)| file_kind(base) == kind)
            .map(|(index, _)| index)
            .collect();
        if let ([only], 1) = (same_kind_baseline.as_slice(), same_kind_current.count())
            && !used[*only]
        {
            used[*only] = true;
            matched[position] = Some(*only);
        }
    }

    current
        .iter()
        .zip(matched)
        .map(|(artifact, index)| {
            let base = index.map(|index| &baseline[index]);
            ArtifactSizeComparison {
                artifact_id: artifact.artifact_id.clone(),
                name: artifact.name.clone(),
                artifact_type: artifact.artifact_type.clone(),
                size_bytes: artifact.size_bytes,
                baseline_name: base.map(|base| base.name.clone()),
                baseline_size_bytes: base.map(|base| base.size_bytes),
                delta_bytes: base.map(|base| artifact.size_bytes - base.size_bytes),
            }
        })
        .collect()
}

/// `24.3 MB`, matching the web app's file sizes.
pub(crate) fn format_size(bytes: i64) -> String {
    const KB: f64 = 1024.0;
    let value = bytes.unsigned_abs() as f64;
    if value < KB {
        format!("{} B", bytes.unsigned_abs())
    } else if value < KB * KB {
        format!("{:.1} KB", value / KB)
    } else if value < KB * KB * KB {
        format!("{:.1} MB", value / (KB * KB))
    } else {
        format!("{:.2} GB", value / (KB * KB * KB))
    }
}

/// `+1.2 MB (+5.1%)`, `-300 B (-0.1%)` or `No change`.
pub(crate) fn format_delta(delta: i64, baseline: i64) -> String {
    if delta == 0 {
        return "No change".to_string();
    }
    let sign = if delta > 0 { '+' } else { '-' };
    let size = format_size(delta);
    if baseline > 0 {
        let percent = delta as f64 / baseline as f64 * 100.0;
        format!("{sign}{size} ({percent:+.1}%)")
    } else {
        format!("{sign}{size}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(id: &str, name: &str, artifact_type: &str, bytes: i64) -> SizeRow {
        SizeRow {
            artifact_id: id.to_string(),
            name: name.to_string(),
            artifact_type: artifact_type.to_string(),
            size_bytes: bytes,
        }
    }

    #[test]
    fn matches_by_name_then_by_the_only_file_of_a_kind() {
        let current = [
            size("a1", "app-release.aab", "generic", 2_000),
            size("a2", "app-1.3.0.apk", "apk", 1_500),
            size("a3", "Runner.ipa", "ipa", 900),
        ];
        let baseline = [
            size("b1", "app-1.2.0.apk", "apk", 1_000),
            size("b2", "app-release.aab", "generic", 2_500),
        ];
        let compared = compare_sizes(&current, &baseline);
        assert_eq!(
            compared[0].baseline_name.as_deref(),
            Some("app-release.aab")
        );
        assert_eq!(compared[0].delta_bytes, Some(-500));
        assert_eq!(compared[1].baseline_name.as_deref(), Some("app-1.2.0.apk"));
        assert_eq!(compared[1].delta_bytes, Some(500));
        assert_eq!(compared[2].baseline_size_bytes, None);
        assert_eq!(compared[2].delta_bytes, None);
    }

    #[test]
    fn several_files_of_a_kind_only_match_by_name() {
        let current = [
            size("a1", "app-arm64-v8a-release.apk", "apk", 10),
            size("a2", "app-x86_64-release.apk", "apk", 20),
        ];
        let baseline = [
            size("b1", "app-armeabi-v7a-release.apk", "apk", 5),
            size("b2", "app-x86_64-release.apk", "apk", 15),
        ];
        let compared = compare_sizes(&current, &baseline);
        assert_eq!(compared[0].delta_bytes, None);
        assert_eq!(compared[1].delta_bytes, Some(5));
    }

    #[test]
    fn formats_sizes_and_changes() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(25_480_396), "24.3 MB");
        assert_eq!(format_delta(1_258_291, 24_222_105), "+1.2 MB (+5.2%)");
        assert_eq!(format_delta(-300, 300_000), "-300 B (-0.1%)");
        assert_eq!(format_delta(0, 10), "No change");
        assert_eq!(format_delta(2_048, 0), "+2.0 KB");
    }

    async fn seeded_pool() -> SqlitePool {
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
             VALUES ('pl1', 'p1', 'Release', 0, 0), ('pl2', 'p1', 'Nightly', 0, 0)",
            "INSERT INTO integrations (id, provider, host_url, auth_mode, created_by, created_at, updated_at) \
             VALUES ('i1', 'github', 'https://github.com', 'github_app', 'u1', 0, 0)",
            "INSERT INTO integration_webhooks (id, integration_id, provider_delivery_id, event_type, payload, received_at) \
             VALUES ('w1', 'i1', 'd1', 'pull_request', \
                 '{\"pull_request\":{\"base\":{\"ref\":\"develop\"}}}', 0)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        pool
    }

    struct Seed<'a> {
        id: &'a str,
        number: i64,
        pipeline: &'a str,
        branch: &'a str,
        status: &'a str,
        event: &'a str,
        webhook: Option<&'a str>,
        source: Option<&'a str>,
        aab_bytes: Option<i64>,
    }

    async fn seed(pool: &SqlitePool, build: Seed<'_>) {
        let expired_from = (build.status == "expired").then_some("succeeded");
        sqlx::query(
            "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
                 trigger_event, branch, webhook_id, source_build_id, expired_from, \
                 queued_at, created_at, updated_at) \
             VALUES (?1, 'p1', ?2, ?3, ?4, 'webhook', ?5, ?6, ?7, ?8, ?9, ?3, ?3, ?3)",
        )
        .bind(build.id)
        .bind(build.pipeline)
        .bind(build.number)
        .bind(build.status)
        .bind(build.event)
        .bind(build.branch)
        .bind(build.webhook)
        .bind(build.source)
        .bind(expired_from)
        .execute(pool)
        .await
        .unwrap();
        if let Some(bytes) = build.aab_bytes {
            let artifact_id = format!("{}-aab", build.id);
            sqlx::query(
                "INSERT INTO artifacts (id, build_id, name, artifact_type, file_path, file_size, \
                     created_at, state) \
                 VALUES (?1, ?2, 'app-release.aab', 'generic', 'k', ?3, 0, 'available')",
            )
            .bind(&artifact_id)
            .bind(build.id)
            .bind(bytes)
            .execute(pool)
            .await
            .unwrap();
            record_artifact_size(pool, &artifact_id).await;
        }
    }

    fn push(
        id: &'static str,
        number: i64,
        branch: &'static str,
        status: &'static str,
        bytes: Option<i64>,
    ) -> Seed<'static> {
        Seed {
            id,
            number,
            pipeline: "pl1",
            branch,
            status,
            event: "push",
            webhook: None,
            source: None,
            aab_bytes: bytes,
        }
    }

    #[tokio::test]
    async fn pull_request_compares_with_its_target_branch() {
        let pool = seeded_pool().await;
        seed(&pool, push("main1", 1, "main", "succeeded", Some(1_000))).await;
        seed(&pool, push("dev1", 2, "develop", "expired", Some(2_000))).await;
        seed(&pool, push("dev2", 3, "develop", "failed", Some(9_000))).await;
        seed(
            &pool,
            Seed {
                pipeline: "pl2",
                ..push("other", 4, "develop", "succeeded", Some(7_000))
            },
        )
        .await;
        seed(
            &pool,
            Seed {
                event: "pull_request",
                webhook: Some("w1"),
                ..push("pr", 5, "feature/login", "succeeded", Some(2_500))
            },
        )
        .await;
        // Built later on the target branch, so not a baseline for build 5.
        seed(&pool, push("dev3", 6, "develop", "succeeded", Some(1))).await;

        let report = load_size_report(&pool, "pr").await.unwrap().unwrap();
        assert_eq!(report.base_branch.as_deref(), Some("develop"));
        let baseline = report.baseline.unwrap();
        assert_eq!(baseline.build_id, "dev1");
        assert_eq!(report.artifacts.len(), 1);
        assert_eq!(report.artifacts[0].size_bytes, 2_500);
        assert_eq!(report.artifacts[0].delta_bytes, Some(500));

        // Retention deleting the baseline's files keeps its recorded sizes.
        sqlx::query("DELETE FROM artifacts WHERE build_id = 'dev1'")
            .execute(&pool)
            .await
            .unwrap();
        let report = load_size_report(&pool, "pr").await.unwrap().unwrap();
        assert_eq!(report.artifacts[0].baseline_size_bytes, Some(2_000));
    }

    #[tokio::test]
    async fn pull_requests_from_a_branch_named_main_are_never_the_baseline() {
        let pool = seeded_pool().await;
        sqlx::query(
            "INSERT INTO integration_webhooks (id, integration_id, provider_delivery_id, event_type, payload, received_at) \
             VALUES ('w2', 'i1', 'd2', 'pull_request', \
                 '{\"pull_request\":{\"base\":{\"ref\":\"release\"}}}', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        seed(&pool, push("main1", 1, "main", "succeeded", Some(1_000))).await;
        // A fork's pull request whose head branch is also called `main`.
        let fork_pr = Seed {
            event: "pull_request",
            webhook: Some("w2"),
            ..push("fork", 2, "main", "succeeded", Some(9_000))
        };
        seed(&pool, fork_pr).await;
        // A rerun of that pull request, then a rerun of the rerun.
        let rerun = Seed {
            event: "rerun",
            source: Some("fork"),
            ..push("rerun1", 3, "main", "succeeded", Some(9_100))
        };
        seed(&pool, rerun).await;
        let rerun_of_rerun = Seed {
            event: "retry",
            source: Some("rerun1"),
            ..push("rerun2", 4, "main", "succeeded", Some(9_200))
        };
        seed(&pool, rerun_of_rerun).await;
        seed(&pool, push("main5", 5, "main", "succeeded", Some(1_100))).await;

        let report = load_size_report(&pool, "main5").await.unwrap().unwrap();
        assert_eq!(report.baseline.unwrap().build_id, "main1");
        assert_eq!(report.artifacts[0].delta_bytes, Some(100));

        // The rerun of a rerun still compares with the pull request's target.
        let report = load_size_report(&pool, "rerun2").await.unwrap().unwrap();
        assert_eq!(report.base_branch.as_deref(), Some("release"));
        assert!(report.baseline.is_none());
    }

    #[tokio::test]
    async fn push_compares_with_the_default_branch_and_may_have_no_baseline() {
        let pool = seeded_pool().await;
        seed(&pool, push("main1", 1, "main", "succeeded", Some(1_000))).await;
        seed(&pool, push("main2", 2, "main", "succeeded", None)).await;
        seed(&pool, push("main3", 3, "main", "succeeded", Some(1_200))).await;

        let report = load_size_report(&pool, "main3").await.unwrap().unwrap();
        assert_eq!(report.base_branch.as_deref(), Some("main"));
        assert_eq!(report.baseline.unwrap().build_number, 1);
        assert_eq!(report.artifacts[0].delta_bytes, Some(200));

        let first = load_size_report(&pool, "main1").await.unwrap().unwrap();
        assert!(first.baseline.is_none());
        assert_eq!(first.artifacts[0].delta_bytes, None);
        assert!(load_size_report(&pool, "missing").await.unwrap().is_none());
    }
}
