// Build concurrency tests — verifies atomic build_number allocation under load.
// Run with: cargo test -p oored --features test-support
#![cfg(feature = "test-support")]

mod common;

use std::collections::HashSet;

use sqlx::Row;
use tokio::task::JoinSet;

#[tokio::test]
async fn runner_claim_query_uses_queue_order_index() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let _app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;

    let rows = sqlx::query(
        "EXPLAIN QUERY PLAN \
         SELECT b.* FROM builds b \
         JOIN projects p ON p.id = b.project_id \
         JOIN integration_repositories r ON r.id = p.repository_id \
         JOIN instance_preferences pref ON pref.id = 1 \
         WHERE b.status = 'queued' \
           AND pref.direct_macos_runner_enabled = 1 \
           AND r.allow_direct_macos_runner = 1 \
         ORDER BY b.queued_at ASC, b.id ASC LIMIT 1",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let details = rows
        .iter()
        .map(|row| row.get::<String, _>("detail"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !details.contains("USE TEMP B-TREE FOR ORDER BY"),
        "claim query should use idx_builds_claim_queue:\n{details}"
    );
}

#[tokio::test]
async fn test_concurrent_build_number_allocation() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let _app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;

    let user_id = common::seed_test_user(&pool).await;
    let secret = "gh-concurrency-secret";
    let integration_id = common::seed_github_integration(&pool, &user_id, secret).await;
    let (_project_id, _pipeline_id) =
        common::seed_project_chain(&pool, &integration_id, &user_id, "org/concurrent-repo").await;

    // Pre-seed webhook records (trigger_build_from_webhook uses webhook_id as FK)
    let mut webhook_ids = Vec::new();
    for i in 0..20u32 {
        let wid =
            common::seed_webhook_record(&pool, &integration_id, &format!("delivery-conc-{i}"))
                .await;
        webhook_ids.push(wid);
    }

    // Fire 20 concurrent webhook-style build triggers
    let mut set = JoinSet::new();
    for (i, webhook_id) in webhook_ids.into_iter().enumerate() {
        let pool = pool.clone();
        let integration_id = integration_id.clone();
        set.spawn(async move {
            oored::builds::trigger_build_from_webhook(
                &pool,
                &webhook_id,
                &integration_id,
                "org/concurrent-repo",
                Some("main"),
                Some(&format!("sha-{i}")),
                "push",
                Some("test-user"),
            )
            .await
        });
    }

    // Collect results
    let mut build_numbers: Vec<i64> = Vec::new();
    while let Some(result) = set.join_next().await {
        let builds = result
            .expect("task panicked")
            .expect("build trigger failed");
        for b in builds {
            build_numbers.push(b.build_number);
        }
    }

    build_numbers.sort();
    assert_eq!(build_numbers.len(), 20, "expected 20 builds");

    // All build numbers must be unique
    let unique: HashSet<i64> = build_numbers.iter().copied().collect();
    assert_eq!(unique.len(), 20, "build numbers must be unique");

    // Should be sequential 1..=20
    assert_eq!(build_numbers, (1..=20).collect::<Vec<i64>>());
}

#[tokio::test]
async fn test_concurrent_build_numbers_across_projects() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let _app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;

    let user_id = common::seed_test_user(&pool).await;
    let secret = "gh-cross-proj-secret";
    let integration_id = common::seed_github_integration(&pool, &user_id, secret).await;

    // Two separate repos → two separate projects
    let (project_a, _) =
        common::seed_project_chain(&pool, &integration_id, &user_id, "org/proj-a").await;
    let (project_b, _) =
        common::seed_project_chain(&pool, &integration_id, &user_id, "org/proj-b").await;

    // Pre-seed webhook records for all 20 triggers
    let mut webhook_ids_a = Vec::new();
    for i in 0..10u32 {
        let wid =
            common::seed_webhook_record(&pool, &integration_id, &format!("delivery-a-{i}")).await;
        webhook_ids_a.push(wid);
    }
    let mut webhook_ids_b = Vec::new();
    for i in 0..10u32 {
        let wid =
            common::seed_webhook_record(&pool, &integration_id, &format!("delivery-b-{i}")).await;
        webhook_ids_b.push(wid);
    }

    let mut set = JoinSet::new();

    // 10 builds for project A
    for (i, webhook_id) in webhook_ids_a.into_iter().enumerate() {
        let pool = pool.clone();
        let integration_id = integration_id.clone();
        set.spawn(async move {
            oored::builds::trigger_build_from_webhook(
                &pool,
                &webhook_id,
                &integration_id,
                "org/proj-a",
                Some("main"),
                Some(&format!("sha-a-{i}")),
                "push",
                Some("test-user"),
            )
            .await
        });
    }

    // 10 builds for project B
    for (i, webhook_id) in webhook_ids_b.into_iter().enumerate() {
        let pool = pool.clone();
        let integration_id = integration_id.clone();
        set.spawn(async move {
            oored::builds::trigger_build_from_webhook(
                &pool,
                &webhook_id,
                &integration_id,
                "org/proj-b",
                Some("main"),
                Some(&format!("sha-b-{i}")),
                "push",
                Some("test-user"),
            )
            .await
        });
    }

    while let Some(result) = set.join_next().await {
        result
            .expect("task panicked")
            .expect("build trigger failed");
    }

    // Verify project A has builds 1..=10
    let rows_a = sqlx::query(
        "SELECT build_number FROM builds WHERE project_id = ?1 ORDER BY build_number ASC",
    )
    .bind(&project_a)
    .fetch_all(&pool)
    .await
    .unwrap();

    let nums_a: Vec<i64> = rows_a.iter().map(|r| r.get("build_number")).collect();
    assert_eq!(nums_a.len(), 10, "project A should have 10 builds");
    assert_eq!(nums_a, (1..=10).collect::<Vec<i64>>());

    // Verify project B has builds 1..=10 (independent sequence)
    let rows_b = sqlx::query(
        "SELECT build_number FROM builds WHERE project_id = ?1 ORDER BY build_number ASC",
    )
    .bind(&project_b)
    .fetch_all(&pool)
    .await
    .unwrap();

    let nums_b: Vec<i64> = rows_b.iter().map(|r| r.get("build_number")).collect();
    assert_eq!(nums_b.len(), 10, "project B should have 10 builds");
    assert_eq!(nums_b, (1..=10).collect::<Vec<i64>>());
}
