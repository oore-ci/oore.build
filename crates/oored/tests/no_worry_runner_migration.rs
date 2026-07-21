use sqlx::Row;
use sqlx::sqlite::SqlitePoolOptions;

const MIGRATION: &str = include_str!("../migrations/037_no_worry_runner_policy.sql");

async fn old_schema_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();

    sqlx::raw_sql(
        "CREATE TABLE integrations (
            id TEXT PRIMARY KEY,
            provider TEXT NOT NULL,
            host_url TEXT NOT NULL
        );
        CREATE TABLE integration_installations (
            id TEXT PRIMARY KEY,
            integration_id TEXT NOT NULL
        );
        CREATE TABLE integration_repositories (
            id TEXT PRIMARY KEY,
            installation_id TEXT NOT NULL,
            full_name TEXT NOT NULL,
            html_url TEXT,
            allow_direct_macos_runner INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE projects (
            id TEXT PRIMARY KEY,
            repository_id TEXT,
            updated_at INTEGER NOT NULL
        );
        CREATE TABLE builds (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            status TEXT NOT NULL,
            runner_id TEXT,
            signing_token_hash TEXT,
            config_snapshot TEXT NOT NULL DEFAULT '{}',
            finished_at INTEGER,
            updated_at INTEGER NOT NULL
        );
        CREATE TABLE build_events (
            id TEXT PRIMARY KEY,
            build_id TEXT NOT NULL,
            from_status TEXT,
            to_status TEXT NOT NULL,
            actor TEXT NOT NULL,
            reason TEXT,
            created_at INTEGER NOT NULL
        );
        CREATE TABLE instance_preferences (
            id INTEGER PRIMARY KEY,
            direct_macos_runner_enabled INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE audit_logs (
            action TEXT NOT NULL,
            resource_type TEXT NOT NULL,
            resource_id TEXT,
            details TEXT
        );",
    )
    .execute(&pool)
    .await
    .unwrap();

    pool
}

async fn migrate_policy(old_enabled: i64, explicit_disable: bool) -> sqlx::SqlitePool {
    let pool = old_schema_pool().await;

    sqlx::query(
        "INSERT INTO instance_preferences (id, direct_macos_runner_enabled) VALUES (1, ?1)",
    )
    .bind(old_enabled)
    .execute(&pool)
    .await
    .unwrap();

    if explicit_disable {
        sqlx::query(
            "INSERT INTO audit_logs (action, resource_type, resource_id, details)
             VALUES (
               'direct_macos_runner_policy_updated',
               'instance_settings',
               'direct_macos_runner',
               '{\"previous_direct_macos_runner_enabled\":true,\"direct_macos_runner_enabled\":false}'
             )",
        )
        .execute(&pool)
        .await
        .unwrap();
    }

    sqlx::raw_sql(MIGRATION).execute(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn migration_makes_the_old_untouched_default_accept_builds() {
    let pool = migrate_policy(0, false).await;
    let paused: i64 = sqlx::query_scalar(
        "SELECT direct_macos_runner_paused FROM instance_preferences WHERE id = 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(paused, 0);
}

#[tokio::test]
async fn migration_preserves_an_explicit_operator_pause() {
    let pool = migrate_policy(0, true).await;
    let paused: i64 = sqlx::query_scalar(
        "SELECT direct_macos_runner_paused FROM instance_preferences WHERE id = 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(paused, 1);
}

#[tokio::test]
async fn migration_keeps_a_previously_enabled_runner_accepting_builds() {
    let pool = migrate_policy(1, false).await;
    let paused: i64 = sqlx::query_scalar(
        "SELECT direct_macos_runner_paused FROM instance_preferences WHERE id = 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(paused, 0);
}

#[tokio::test]
async fn migration_requires_legacy_unapproved_projects_to_be_retrusted() {
    let pool = old_schema_pool().await;
    sqlx::raw_sql(
        r#"INSERT INTO instance_preferences VALUES (1, 1);
        INSERT INTO integrations VALUES ('gitlab', 'gitlab', 'https://gitlab.example.com');
        INSERT INTO integration_installations VALUES ('install', 'gitlab');
        INSERT INTO integration_repositories VALUES (
          'blocked-repo', 'install', 'acme/blocked', 'https://gitlab.example.com/acme/blocked', 0
        );
        INSERT INTO integration_repositories VALUES (
          'allowed-repo', 'install', 'acme/allowed', 'https://gitlab.example.com/acme/allowed', 1
        );
        INSERT INTO projects VALUES ('blocked-project', 'blocked-repo', 1);
        INSERT INTO projects VALUES ('allowed-project', 'allowed-repo', 1);
        INSERT INTO builds VALUES (
          'blocked-build', 'blocked-project', 'queued', NULL, 'grant',
          '{"repo_url":"https://gitlab.example.com/acme/blocked.git"}', NULL, 1
        );
        INSERT INTO builds VALUES (
          'allowed-build', 'allowed-project', 'queued', NULL, 'grant',
          '{"repo_url":"https://gitlab.example.com/acme/allowed.git"}', NULL, 1
        );
        INSERT INTO builds VALUES (
          'running-blocked-build', 'blocked-project', 'running', 'runner', 'active-grant',
          '{"repo_url":"https://gitlab.example.com/acme/blocked.git"}', NULL, 1
        );"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::raw_sql(MIGRATION).execute(&pool).await.unwrap();

    let rows = sqlx::query(
        "SELECT id, status, signing_token_hash, config_snapshot FROM builds ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows[0].get::<String, _>("id"), "allowed-build");
    assert_eq!(rows[0].get::<String, _>("status"), "queued");
    assert_eq!(rows[1].get::<String, _>("id"), "blocked-build");
    assert_eq!(rows[1].get::<String, _>("status"), "canceled");
    assert!(
        rows[1]
            .get::<Option<String>, _>("signing_token_hash")
            .is_none()
    );
    assert_eq!(rows[2].get::<String, _>("id"), "running-blocked-build");
    assert_eq!(rows[2].get::<String, _>("status"), "running");
    let running_snapshot: serde_json::Value =
        serde_json::from_str(&rows[2].get::<String, _>("config_snapshot")).unwrap();
    assert_eq!(
        running_snapshot["repository_id"].as_str(),
        Some("blocked-repo")
    );

    let events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM build_events WHERE build_id = 'blocked-build' AND to_status = 'canceled'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(events, 1);

    let project_sources = sqlx::query("SELECT id, repository_id FROM projects ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(project_sources[0].get::<String, _>("id"), "allowed-project");
    assert_eq!(
        project_sources[0]
            .get::<Option<String>, _>("repository_id")
            .as_deref(),
        Some("allowed-repo")
    );
    assert_eq!(project_sources[1].get::<String, _>("id"), "blocked-project");
    assert!(
        project_sources[1]
            .get::<Option<String>, _>("repository_id")
            .is_none()
    );

    let draining_source: String = sqlx::query_scalar(
        "SELECT r.full_name
         FROM builds b
         JOIN integration_repositories r
           ON r.id = json_extract(b.config_snapshot, '$.repository_id')
         WHERE b.id = 'running-blocked-build'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(draining_source, "acme/blocked");
}

#[tokio::test]
async fn migration_preserves_a_relinked_running_builds_original_gitlab_source() {
    let pool = old_schema_pool().await;
    sqlx::raw_sql(
        r#"INSERT INTO instance_preferences VALUES (1, 1);
        INSERT INTO integrations VALUES ('gitlab', 'gitlab', 'https://gitlab.example.com');
        INSERT INTO integration_installations VALUES ('install', 'gitlab');
        INSERT INTO integration_repositories VALUES (
          'source-a', 'install', 'acme/source-a', 'https://gitlab.example.com/acme/source-a', 1
        );
        INSERT INTO integration_repositories VALUES (
          'source-b', 'install', 'acme/source-b', 'https://gitlab.example.com/acme/source-b', 1
        );
        INSERT INTO projects VALUES ('project', 'source-b', 1);
        INSERT INTO projects VALUES ('unlinked-project', NULL, 1);
        INSERT INTO builds VALUES (
          'running-from-a', 'project', 'running', 'runner', 'active-grant',
          '{"repo_url":"https://gitlab.example.com/acme/source-a.git"}', NULL, 1
        );
        INSERT INTO builds VALUES (
          'queued-from-a-unlinked', 'unlinked-project', 'queued', NULL, NULL,
          '{"repo_url":"https://gitlab.example.com/acme/source-a.git"}', NULL, 1
        );"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::raw_sql(MIGRATION).execute(&pool).await.unwrap();

    let project_source: String =
        sqlx::query_scalar("SELECT repository_id FROM projects WHERE id = 'project'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(project_source, "source-b");
    let row = sqlx::query("SELECT status, config_snapshot FROM builds WHERE id = 'running-from-a'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>("status"), "running");
    let snapshot: serde_json::Value =
        serde_json::from_str(&row.get::<String, _>("config_snapshot")).unwrap();
    assert_eq!(snapshot["repository_id"].as_str(), Some("source-a"));
    let unlinked_status: String =
        sqlx::query_scalar("SELECT status FROM builds WHERE id = 'queued-from-a-unlinked'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(unlinked_status, "canceled");
}
