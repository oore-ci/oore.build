use sqlx::SqlitePool;

use super::*;
use crate::crypto;

const KEY: [u8; 32] = [7u8; 32];

async fn pool_before_stores() -> (SqlitePool, sqlx::migrate::Migrator) {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrator = sqlx::migrate!("./migrations");
    // Everything before 100_project_store_connections.sql.
    migrator.run_to(99, &pool).await.unwrap();
    (pool, migrator)
}

async fn exec(pool: &SqlitePool, statements: &[&str]) {
    for statement in statements {
        sqlx::query(sqlx::AssertSqlSafe(statement.to_string()))
            .execute(pool)
            .await
            .unwrap();
    }
}

async fn insert_signing(
    pool: &SqlitePool,
    pipeline_id: &str,
    key_id: &str,
    shared_key_id: Option<&str>,
    updated_at: i64,
) {
    sqlx::query(
        "INSERT INTO pipeline_ios_signing_settings
            (id, pipeline_id, enabled, mode, export_method, api_key_id, api_issuer_id,
             api_private_key_encrypted, shared_api_key_id, created_at, updated_at)
         VALUES (?1, ?1, 1, 'api', 'app_store', ?2, 'issuer', ?3, ?4, 0, ?5)",
    )
    .bind(pipeline_id)
    .bind(key_id)
    .bind(crypto::encrypt(&format!("{key_id}-pem"), &KEY).unwrap())
    .bind(shared_key_id)
    .bind(updated_at)
    .execute(pool)
    .await
    .unwrap();
}

fn play_target(id: &str, pipeline: &str, enabled: bool, config: &str, updated_at: i64) -> String {
    format!(
        "INSERT INTO pipeline_publish_targets (id, pipeline_id, store, enabled, config, created_at, updated_at) \
         VALUES ('{id}', '{pipeline}', 'google_play', {}, '{config}', 0, {updated_at})",
        i32::from(enabled)
    )
}

async fn seed_apps(pool: &SqlitePool) {
    exec(
        pool,
        &[
            "INSERT INTO users (id, email, oidc_subject, role, created_at, updated_at) \
             VALUES ('u1', 'lead@example.com', 'sub', 'owner', 0, 0)",
            "INSERT INTO projects (id, name, created_by, created_at, updated_at) VALUES \
             ('p1', 'Shop', 'u1', 0, 0), ('p2', 'Bank', 'u1', 0, 0), ('p3', 'Quiet', 'u1', 0, 0)",
            "INSERT INTO pipelines (id, project_id, name, created_at, updated_at) VALUES \
             ('a1', 'p1', 'Play beta', 0, 0), ('a2', 'p1', 'Play prod', 0, 0), \
             ('a3', 'p1', 'Wear', 0, 0), ('a4', 'p1', 'iOS', 0, 0), ('a5', 'p1', 'iOS ad hoc', 0, 0), \
             ('b1', 'p2', 'Bank iOS', 0, 0), ('b2', 'p2', 'Bank iOS beta', 0, 0), \
             ('c1', 'p3', 'Nothing', 0, 0)",
        ],
    )
    .await;
    // Shop: the enabled pipeline wins over a more recently updated disabled one.
    exec(
        pool,
        &[
            &play_target(
                "t1",
                "a1",
                false,
                r#"{"service_account_secret":"PLAY_OLD","track":"internal"}"#,
                90,
            ),
            &play_target(
                "t2",
                "a2",
                true,
                r#"{"service_account_secret":"PLAY","package_name":"com.shop","track":"production"}"#,
                50,
            ),
            &play_target(
                "t3",
                "a3",
                true,
                r#"{"service_account_secret":"PLAY","package_name":"com.shop.wear","track":"internal"}"#,
                10,
            ),
        ],
    )
    .await;
    // Shop iOS: two pipelines with their own, different keys; the one that
    // uploads wins although the other was updated later.
    insert_signing(pool, "a4", "KEYA", None, 10).await;
    insert_signing(pool, "a5", "KEYB", None, 99).await;
    exec(
        pool,
        &["INSERT INTO pipeline_publish_targets (id, pipeline_id, store, enabled, config, created_at, updated_at) \
           VALUES ('t4', 'a4', 'app_store', 1, \
           '{\"enabled\":true,\"destination\":\"testflight\",\"testflight_groups\":[\"QA\"],\"release_notes\":[]}', 0, 0)"],
    )
    .await;
    // Bank: one pipeline uses a shared key, the other its own copy of the
    // same key; the shared key is reused, nothing is copied.
    sqlx::query(
        "INSERT INTO app_store_connect_keys
            (id, name, key_id, issuer_id, private_key_encrypted, created_at, updated_at)
         VALUES ('shared', 'Team key', 'KEYS', 'issuer', ?1, 0, 0)",
    )
    .bind(crypto::encrypt("KEYS-pem", &KEY).unwrap())
    .execute(pool)
    .await
    .unwrap();
    insert_signing(pool, "b1", "IGNORED", Some("shared"), 5).await;
    insert_signing(pool, "b2", "KEYS", None, 50).await;
}

async fn config_of(pool: &SqlitePool, pipeline: &str, store: &str) -> serde_json::Value {
    let raw: String = sqlx::query_scalar(
        "SELECT config FROM pipeline_publish_targets WHERE pipeline_id = ?1 AND store = ?2",
    )
    .bind(pipeline)
    .bind(store)
    .fetch_one(pool)
    .await
    .unwrap();
    serde_json::from_str(&raw).unwrap()
}

#[tokio::test]
async fn the_migration_connects_each_app_once_and_keeps_disagreeing_pipelines() {
    let (pool, migrator) = pool_before_stores().await;
    seed_apps(&pool).await;
    let keys_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_store_connect_keys")
        .fetch_one(&pool)
        .await
        .unwrap();
    migrator.run_to(100, &pool).await.unwrap();

    // Google Play: the enabled pipeline's secret and package.
    let play = google_play_connection(&pool, "p1").await.unwrap().unwrap();
    assert_eq!(play.service_account_secret, "PLAY");
    assert_eq!(play.package_name.as_deref(), Some("com.shop"));
    let migrated_from: String = sqlx::query_scalar(
        "SELECT migrated_from_pipeline_id FROM project_store_connections \
         WHERE project_id = 'p1' AND store = 'google_play'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(migrated_from, "a2");
    // Matching pipelines follow the app; their other settings stay.
    let a2 = config_of(&pool, "a2", "google_play").await;
    assert_eq!(a2["service_account_secret"], "");
    assert!(a2.get("package_name").is_none());
    assert_eq!(a2["track"], "production");
    let a3 = config_of(&pool, "a3", "google_play").await;
    assert_eq!(a3["service_account_secret"], "");
    assert_eq!(a3["package_name"], "com.shop.wear");
    // A different secret stays as the pipeline's own override.
    let a1 = config_of(&pool, "a1", "google_play").await;
    assert_eq!(a1["service_account_secret"], "PLAY_OLD");
    // An app with no publishing gets no connection.
    assert!(google_play_connection(&pool, "p3").await.unwrap().is_none());

    // App Store Connect: own keys were copied into stored keys, once each.
    let keys_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_store_connect_keys")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(keys_after, keys_before + 2, "KEYA and KEYB, not KEYS");
    let (shop_key, shop_name): (String, String) = sqlx::query_as(
        "SELECT k.key_id, k.name FROM project_store_connections c \
         JOIN app_store_connect_keys k ON k.id = c.app_store_connect_key_id \
         WHERE c.project_id = 'p1' AND c.store = 'app_store'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(shop_key, "KEYA");
    assert_eq!(shop_name, "Shop (KEYA)");
    // The copy decrypts to the same private key.
    let copied: String = sqlx::query_scalar(
        "SELECT private_key_encrypted FROM app_store_connect_keys WHERE key_id = 'KEYA'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(crypto::decrypt(&copied, &KEY).unwrap(), "KEYA-pem");
    // a5 uses another key: it gets publish settings, off, that keep it.
    let a5 = config_of(&pool, "a5", "app_store").await;
    assert_eq!(a5["upload_with_signing_key"], true);
    assert_eq!(a5["enabled"], false);
    let a4 = config_of(&pool, "a4", "app_store").await;
    assert!(a4.get("upload_with_signing_key").is_none());
    assert_eq!(a4["testflight_groups"][0], "QA");

    // Bank reuses the shared key, and both pipelines agree with it.
    let bank_key: String = sqlx::query_scalar(
        "SELECT app_store_connect_key_id FROM project_store_connections \
         WHERE project_id = 'p2' AND store = 'app_store'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(bank_key, "shared");
    let bank_overrides: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pipeline_publish_targets WHERE pipeline_id IN ('b1', 'b2')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(bank_overrides, 0);

    // The pipelines' own keys stay stored for signing.
    let own_keys: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pipeline_ios_signing_settings WHERE api_private_key_encrypted IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(own_keys, 4);

    let overrides = overrides(&pool, "p1", Some("PLAY"), true).await.unwrap();
    let names: Vec<_> = overrides
        .iter()
        .map(|o| (o.pipeline_name.as_str(), o.store))
        .collect();
    assert_eq!(
        names,
        vec![
            ("Play beta", PublishStore::GooglePlay),
            ("iOS ad hoc", PublishStore::AppStore)
        ]
    );

    assert!(
        sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&pool)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn uploads_resolve_through_the_app_connection_with_a_fallback() {
    let (pool, migrator) = pool_before_stores().await;
    seed_apps(&pool).await;
    migrator.run_to(100, &pool).await.unwrap();

    // Google Play: an empty secret and missing package come from the app.
    let (a2, _) = crate::pipeline_publish::load_google_play(&pool, "a2")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(a2.service_account_secret, "PLAY");
    assert_eq!(a2.package_name.as_deref(), Some("com.shop"));
    let (a3, _) = crate::pipeline_publish::load_google_play(&pool, "a3")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(a3.package_name.as_deref(), Some("com.shop.wear"));
    let (a1, _) = crate::pipeline_publish::load_google_play(&pool, "a1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(a1.service_account_secret, "PLAY_OLD");
    // Changing the app's secret moves every following pipeline.
    sqlx::query(
        "UPDATE project_store_connections SET service_account_secret = 'PLAY_NEW' \
         WHERE project_id = 'p1' AND store = 'google_play'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let (a2, _) = crate::pipeline_publish::load_google_play(&pool, "a2")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(a2.service_account_secret, "PLAY_NEW");

    // App Store Connect: the app's key, or the signing key when overridden.
    let creds = |pipeline: &'static str| {
        let pool = pool.clone();
        async move {
            crate::app_store_publish::upload_credentials(&pool, &KEY, pipeline)
                .await
                .unwrap()
                .unwrap()
                .key_id
        }
    };
    assert_eq!(creds("a4").await, "KEYA");
    assert_eq!(creds("a5").await, "KEYB");
    // Point the app at another key: a4 follows, a5 keeps its own.
    sqlx::query(
        "UPDATE project_store_connections SET app_store_connect_key_id = 'shared' \
         WHERE project_id = 'p1' AND store = 'app_store'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(creds("a4").await, "KEYS");
    assert_eq!(creds("a5").await, "KEYB");
    // Without a connection, pipelines fall back to their signing key.
    sqlx::query("DELETE FROM project_store_connections WHERE project_id = 'p1'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(creds("a4").await, "KEYA");
    let (a2, _) = crate::pipeline_publish::load_google_play(&pool, "a2")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(a2.service_account_secret, "", "nothing left to resolve to");
}

#[test]
fn package_and_bundle_ids_are_checked() {
    assert!(valid_package("com.example.app"));
    assert!(!valid_package("example"));
    assert!(!valid_package("com.1example"));
    assert!(valid_bundle_id("com.example.app-beta"));
    assert!(!valid_bundle_id("com example"));
    assert!(!valid_bundle_id("example"));
}
