#![cfg(feature = "test-support")]

mod common;

use common::{
    connect_pool, create_test_app, now_unix, seed_github_integration, seed_project_chain,
    seed_test_user,
};

async fn create_queued_build(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    pipeline_id: &str,
) -> String {
    let build_id = uuid::Uuid::new_v4().to_string();
    let now = now_unix();

    sqlx::query(
        "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, \
         trigger_type, config_snapshot, queued_at, created_at, updated_at) \
         VALUES (?1, ?2, ?3, \
                 (SELECT COALESCE(MAX(build_number), 0) + 1 FROM builds WHERE project_id = ?2), \
                 'queued', 'manual', '{}', ?4, ?4, ?4)",
    )
    .bind(&build_id)
    .bind(project_id)
    .bind(pipeline_id)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to create queued build");

    sqlx::query(
        "INSERT INTO build_events (id, build_id, from_status, to_status, actor, reason, created_at) \
         VALUES (?1, ?2, NULL, 'queued', 'test', 'test build', ?3)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&build_id)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to insert queued build event");

    build_id
}

#[tokio::test]
async fn default_runner_mode_does_not_execute_repository_code_in_process() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("embedded_runner_flow.db");

    let _app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    let user_id = seed_test_user(&pool).await;
    let integration_id = seed_github_integration(&pool, &user_id, "whsec-test").await;
    let (project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &user_id, "embedded/test-repo").await;
    let build_id = create_queued_build(&pool, &project_id, &pipeline_id).await;

    let embedded =
        oored::embedded_runner::start_if_enabled(pool.clone(), "http://127.0.0.1:0".to_string())
            .await
            .expect("default runner mode is safe");

    assert!(
        embedded.is_none(),
        "default mode must not start an in-process runner"
    );
    let embedded_runner_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM runners WHERE registered_by IS NULL")
            .fetch_one(&pool)
            .await
            .expect("query embedded runner count");
    assert_eq!(embedded_runner_count, 0);
    let status: String = sqlx::query_scalar("SELECT status FROM builds WHERE id = ?1")
        .bind(&build_id)
        .fetch_one(&pool)
        .await
        .expect("query build status");
    assert_eq!(
        status, "queued",
        "repository job must remain for an external runner"
    );
}
