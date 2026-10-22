//! Guards against starting with a lost or replaced encryption key.
//!
//! Every credential at rest is encrypted with one instance key. If the key
//! file goes missing, silently generating a new one makes every stored
//! credential unreadable, and anything written afterwards uses the new key,
//! leaving the database with mixed keys. The daemon therefore keeps a known
//! value encrypted with the key in `encryption_key_check` and refuses to
//! start when the key on disk cannot decrypt it.

use std::path::PathBuf;

use anyhow::{Context, bail};
use oore_contract::KeyStorageMode;
use sqlx::{Row, SqlitePool};
use tracing::info;

use crate::crypto::{self, RuntimeKey};
use crate::util::now_unix;

use oore_contract::data_paths::ENCRYPTION_KEY_CHECK_PLAINTEXT as KEY_CHECK_PLAINTEXT;

/// Newest values per encrypted column to try when a database predates the
/// check value.
const SAMPLES_PER_COLUMN: i64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCheck {
    /// The key decrypted the stored check value.
    Verified,
    /// The database had no check value, so one was written with this key.
    Recorded,
}

/// Load the runtime key from `key_path` and prove it matches the database.
///
/// A key is generated only when the database holds no encrypted data. The
/// returned key is safe to encrypt with: it either decrypted the stored check
/// value or, for a database that predates it, at least one existing
/// ciphertext.
pub async fn load_verified_runtime_key(
    pool: &SqlitePool,
    mode: KeyStorageMode,
    key_path: PathBuf,
) -> anyhow::Result<(RuntimeKey, KeyCheck)> {
    let allow_generate = !database_has_encrypted_data(pool).await?;
    let runtime_key = crypto::load_runtime_key_with_mode(mode, key_path, allow_generate)?;
    let check = verify_or_record(pool, &runtime_key.key)
        .await
        .with_context(|| {
            format!(
                "encryption key {} does not match this database",
                runtime_key.legacy_file_path.display()
            )
        })?;
    Ok((runtime_key, check))
}

/// Whether the database holds the check value or any ciphertext.
pub async fn database_has_encrypted_data(pool: &SqlitePool) -> anyhow::Result<bool> {
    if load_check_ciphertext(pool).await?.is_some() {
        return Ok(true);
    }
    for (table, column) in encrypted_columns(pool).await? {
        let sql = format!(
            "SELECT EXISTS (SELECT 1 FROM {} WHERE {} IS NOT NULL AND {} != '')",
            quote_ident(&table),
            quote_ident(&column),
            quote_ident(&column),
        );
        let exists: bool = // Identifiers come from sqlite_master and are quoted.
        sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
            .fetch_one(pool)
            .await
            .with_context(|| format!("failed to inspect {table}.{column}"))?;
        if exists {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Check `key` against the stored check value, writing one when absent.
///
/// For a database that predates the check value, the key must decrypt at
/// least one existing ciphertext before the check value is written.
pub async fn verify_or_record(pool: &SqlitePool, key: &[u8]) -> anyhow::Result<KeyCheck> {
    if let Some(check) = load_check_ciphertext(pool).await? {
        if check_value_matches(&check, key) {
            return Ok(KeyCheck::Verified);
        }
        bail!(
            "the key cannot decrypt the stored key check value. Restore the original \
             encryption.key with `oore backup restore`, or set OORED_DATA_DIR to the directory \
             that holds it"
        );
    }

    let samples = sample_ciphertexts(pool).await?;
    if !samples.is_empty()
        && !samples
            .iter()
            .any(|value| crypto::decrypt(value, key).is_ok())
    {
        bail!(
            "the key cannot decrypt any of the {} stored credentials sampled. Restore the \
             original encryption.key with `oore backup restore`, or set OORED_DATA_DIR to the \
             directory that holds it",
            samples.len()
        );
    }

    let ciphertext = crypto::encrypt(KEY_CHECK_PLAINTEXT, key)?;
    sqlx::query(
        "INSERT INTO encryption_key_check (id, check_ciphertext, created_at)
         VALUES (1, ?1, ?2)
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(&ciphertext)
    .bind(now_unix())
    .execute(pool)
    .await
    .context("failed to record the encryption key check value")?;

    // Another process may have recorded a check value first. Trust only what
    // is stored.
    let stored = load_check_ciphertext(pool)
        .await?
        .context("encryption key check value was not recorded")?;
    if !check_value_matches(&stored, key) {
        bail!("another process recorded a key check value for a different key");
    }
    info!("recorded encryption key check value");
    Ok(KeyCheck::Recorded)
}

/// Whether `key` decrypts the stored check value. A database without one
/// counts as a match, since startup records it before serving.
pub async fn key_matches_database(pool: &SqlitePool, key: &[u8]) -> anyhow::Result<bool> {
    Ok(match load_check_ciphertext(pool).await? {
        Some(check) => check_value_matches(&check, key),
        None => true,
    })
}

fn check_value_matches(ciphertext: &str, key: &[u8]) -> bool {
    crypto::decrypt(ciphertext, key).is_ok_and(|plaintext| plaintext == KEY_CHECK_PLAINTEXT)
}

async fn load_check_ciphertext(pool: &SqlitePool) -> anyhow::Result<Option<String>> {
    sqlx::query_scalar("SELECT check_ciphertext FROM encryption_key_check WHERE id = 1")
        .fetch_optional(pool)
        .await
        .context("failed to load the encryption key check value")
}

/// Every `(table, column)` whose column name marks it as holding ciphertext.
async fn encrypted_columns(pool: &SqlitePool) -> anyhow::Result<Vec<(String, String)>> {
    let rows = sqlx::query(
        "SELECT m.name AS table_name, c.name AS column_name
         FROM sqlite_master m, pragma_table_info(m.name) c
         WHERE m.type = 'table'
           AND m.name NOT LIKE 'sqlite_%'
           AND m.name NOT LIKE '_sqlx_%'
           AND c.name LIKE '%encrypted%'
         ORDER BY m.name, c.cid",
    )
    .fetch_all(pool)
    .await
    .context("failed to list encrypted columns")?;
    rows.iter()
        .map(|row| Ok((row.try_get("table_name")?, row.try_get("column_name")?)))
        .collect()
}

async fn sample_ciphertexts(pool: &SqlitePool) -> anyhow::Result<Vec<String>> {
    let mut samples = Vec::new();
    for (table, column) in encrypted_columns(pool).await? {
        // Newest rows first: after an earlier key loss they are the most
        // likely to have been written with the current key.
        let sql = format!(
            "SELECT {col} FROM {table} WHERE {col} IS NOT NULL AND {col} != ''
             ORDER BY rowid DESC LIMIT {SAMPLES_PER_COLUMN}",
            col = quote_ident(&column),
            table = quote_ident(&table),
        );
        let values: Vec<String> = // Identifiers come from sqlite_master and are quoted.
        sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
            .fetch_all(pool)
            .await
            .with_context(|| format!("failed to sample {table}.{column}"))?;
        samples.extend(values);
    }
    Ok(samples)
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for statement in [
            "CREATE TABLE users (id TEXT PRIMARY KEY)",
            "CREATE TABLE projects (id TEXT PRIMARY KEY)",
            "CREATE TABLE pipelines (id TEXT PRIMARY KEY)",
            include_str!("../migrations/042_secrets.sql"),
            include_str!("../migrations/059_encryption_key_check.sql"),
        ] {
            sqlx::raw_sql(statement).execute(&pool).await.unwrap();
        }
        pool
    }

    async fn insert_secret(pool: &SqlitePool, key: &[u8]) {
        sqlx::query(
            "INSERT INTO secrets (id, scope, name, value_encrypted, created_at, updated_at)
             VALUES ('s1', 'instance', 'PLAY', ?1, 0, 0)",
        )
        .bind(crypto::encrypt("{}", key).unwrap())
        .execute(pool)
        .await
        .unwrap();
    }

    fn key_path(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("encryption.key")
    }

    async fn start(pool: &SqlitePool, path: PathBuf) -> anyhow::Result<(RuntimeKey, KeyCheck)> {
        load_verified_runtime_key(pool, KeyStorageMode::File, path).await
    }

    #[tokio::test]
    async fn fresh_install_generates_and_records_a_key() {
        let pool = pool().await;
        let dir = tempfile::tempdir().unwrap();

        let (runtime_key, check) = start(&pool, key_path(&dir)).await.unwrap();

        assert_eq!(check, KeyCheck::Recorded);
        assert_eq!(std::fs::read(key_path(&dir)).unwrap(), runtime_key.key);
        let (restarted, check) = start(&pool, key_path(&dir)).await.unwrap();
        assert_eq!(check, KeyCheck::Verified);
        assert_eq!(restarted.key, runtime_key.key);
    }

    #[tokio::test]
    async fn missing_key_with_encrypted_data_refuses_without_writing_a_key() {
        let pool = pool().await;
        let dir = tempfile::tempdir().unwrap();
        let (runtime_key, _) = start(&pool, key_path(&dir)).await.unwrap();
        insert_secret(&pool, &runtime_key.key).await;
        std::fs::remove_file(key_path(&dir)).unwrap();

        let error = start(&pool, key_path(&dir)).await.unwrap_err();

        let message = format!("{error:#}");
        assert!(message.contains(&key_path(&dir).display().to_string()));
        assert!(message.contains("oore backup restore"));
        assert!(!key_path(&dir).exists());
    }

    #[tokio::test]
    async fn missing_key_on_a_legacy_database_with_ciphertext_refuses() {
        let pool = pool().await;
        insert_secret(&pool, &[1u8; 32]).await;
        let dir = tempfile::tempdir().unwrap();

        assert!(start(&pool, key_path(&dir)).await.is_err());
        assert!(!key_path(&dir).exists());
    }

    #[tokio::test]
    async fn wrong_key_refuses() {
        let pool = pool().await;
        let dir = tempfile::tempdir().unwrap();
        start(&pool, key_path(&dir)).await.unwrap();
        std::fs::write(key_path(&dir), [9u8; 32]).unwrap();

        let error = start(&pool, key_path(&dir)).await.unwrap_err();

        assert!(format!("{error:#}").contains("does not match this database"));
    }

    #[tokio::test]
    async fn empty_database_has_no_encrypted_data_and_records_a_check() {
        let pool = pool().await;
        assert!(!database_has_encrypted_data(&pool).await.unwrap());

        let key = [1u8; 32];
        assert_eq!(
            verify_or_record(&pool, &key).await.unwrap(),
            KeyCheck::Recorded
        );
        assert!(database_has_encrypted_data(&pool).await.unwrap());
        assert_eq!(
            verify_or_record(&pool, &key).await.unwrap(),
            KeyCheck::Verified
        );
        assert!(key_matches_database(&pool, &key).await.unwrap());
    }

    #[tokio::test]
    async fn stored_check_rejects_a_different_key() {
        let pool = pool().await;
        verify_or_record(&pool, &[1u8; 32]).await.unwrap();

        let error = verify_or_record(&pool, &[2u8; 32]).await.unwrap_err();
        assert!(error.to_string().contains("oore backup restore"));
        assert!(!key_matches_database(&pool, &[2u8; 32]).await.unwrap());
    }

    #[tokio::test]
    async fn legacy_database_is_checked_against_existing_ciphertext() {
        let pool = pool().await;
        insert_secret(&pool, &[1u8; 32]).await;
        assert!(database_has_encrypted_data(&pool).await.unwrap());

        assert!(verify_or_record(&pool, &[2u8; 32]).await.is_err());
        assert!(load_check_ciphertext(&pool).await.unwrap().is_none());

        assert_eq!(
            verify_or_record(&pool, &[1u8; 32]).await.unwrap(),
            KeyCheck::Recorded
        );
    }
}
