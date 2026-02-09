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
async fn embedded_runner_claims_queued_build_without_external_runner_process() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("embedded_runner_flow.db");

    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    let user_id = seed_test_user(&pool).await;
    let integration_id = seed_github_integration(&pool, &user_id, "whsec-test").await;
    let (project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &user_id, "embedded/test-repo").await;
    let build_id = create_queued_build(&pool, &project_id, &pipeline_id).await;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let addr = listener.local_addr().expect("listener addr");
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("test server should run");
    });

    let embedded = oored::embedded_runner::start_if_enabled(
        pool.clone(),
        format!("http://127.0.0.1:{}", addr.port()),
    )
    .await
    .expect("embedded runner starts")
    .expect("embedded mode should be enabled by default");

    let mut runner_status = "offline".to_string();
    for _ in 0..8 {
        runner_status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM runners WHERE registered_by IS NULL LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .expect("query embedded runner status");

        if runner_status == "online" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    assert_eq!(
        runner_status, "online",
        "embedded runner should report online shortly after startup"
    );

    let mut final_status = "queued".to_string();
    for _ in 0..25 {
        final_status = sqlx::query_scalar::<_, String>("SELECT status FROM builds WHERE id = ?1")
            .bind(&build_id)
            .fetch_one(&pool)
            .await
            .expect("query build status");

        if final_status != "queued" {
            break;
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    assert_ne!(
        final_status, "queued",
        "embedded runner should claim queued builds"
    );

    embedded.abort();
    server_handle.abort();
}
