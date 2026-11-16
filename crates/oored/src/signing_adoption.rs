//! Moves signing files stored on pipelines into the app's signing library.
//!
//! The library is the only place that holds keystores, certificates and
//! provisioning profiles. Pipelines that still carry their own files (saved
//! before the library existed, or uploaded through the older pipeline
//! signing API) are adopted: their material is copied, still encrypted with
//! the instance key, into a library identity and the pipeline is linked to
//! it, so builds keep signing with the same key and nothing is re-entered.
//!
//! - The same material on several pipelines of one app becomes one identity
//!   (matched by [`content_hash`]), including an identity already in the
//!   library.
//! - A new identity is named after the first pipeline that holds it:
//!   `<pipeline>-keystore`, `<pipeline>-debug-keystore` or
//!   `<pipeline>-certificate`, with `-2`, `-3`, … when the name is taken.
//! - Only enabled signing is adopted, and for iOS only manual mode: API and
//!   hybrid mode fetch profiles from App Store Connect and stay on the
//!   pipeline.
//! - The pipeline's own columns are left as stored.
//!
//! Adoption runs when the daemon starts and after each pipeline signing
//! save. It is idempotent: linked rows are skipped, and a row that cannot be
//! adopted (say, it no longer decrypts) is logged and keeps signing from its
//! own columns.

use oore_contract::{
    IosExportMethod, RunnerIosProvisioningProfile, SigningIdentityKind, SigningIdentityProfile,
    is_valid_workflow_name,
};
use sqlx::{Row, SqlitePool};
use tracing::warn;
use uuid::Uuid;
use zeroize::Zeroize;

use crate::crypto;
use crate::signing_identities::{Material, Summary, content_hash, kind_str};
use crate::util::now_unix;

/// What one adoption pass did.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct AdoptionReport {
    /// Library identities created from pipeline files.
    pub created: usize,
    /// Pipeline signing rows linked to an identity (new or existing).
    pub linked: usize,
    /// Rows left on their own files because they could not be read.
    pub skipped: usize,
}

fn decrypt(value: Option<String>, key: &[u8]) -> anyhow::Result<String> {
    let value = value.ok_or_else(|| anyhow::anyhow!("missing value"))?;
    crypto::decrypt(&value, key)
}

/// A name the library accepts, from a pipeline name: `Coin Release` with
/// `keystore` becomes `coin-release-keystore`.
fn identity_base_name(pipeline_name: &str, suffix: &str) -> String {
    let mut slug = String::new();
    for c in pipeline_name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let mut slug = slug.trim_end_matches('-').to_string();
    if slug.is_empty() {
        slug.push_str("pipeline");
    }
    // Leave room for the suffix and a `-NN` counter within 64 characters.
    slug.truncate(64 - suffix.len() - 5);
    let slug = slug.trim_end_matches('-');
    format!("{slug}-{suffix}")
}

async fn unique_name(pool: &SqlitePool, project_id: &str, base: &str) -> sqlx::Result<String> {
    let mut candidate = base.to_string();
    let mut counter = 2;
    loop {
        let taken: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM project_signing_identities WHERE project_id = ?1 AND name = ?2",
        )
        .bind(project_id)
        .bind(&candidate)
        .fetch_optional(pool)
        .await?;
        if taken.is_none() && is_valid_workflow_name(&candidate) {
            return Ok(candidate);
        }
        candidate = format!("{base}-{counter}");
        counter += 1;
    }
}

/// Fill in the hash of identities saved before hashes existed, so
/// adoption can match pipeline files against them.
async fn backfill_hashes(pool: &SqlitePool, key: &[u8]) -> anyhow::Result<()> {
    let rows = sqlx::query(
        "SELECT id, material_encrypted FROM project_signing_identities WHERE content_hash IS NULL",
    )
    .fetch_all(pool)
    .await?;
    for row in rows {
        let id: String = row.get("id");
        let material = crypto::decrypt(&row.get::<String, _>("material_encrypted"), key).and_then(
            |mut plain| {
                let parsed = serde_json::from_str::<Material>(&plain);
                plain.zeroize();
                Ok(parsed?)
            },
        );
        match material {
            Ok(material) => {
                sqlx::query(
                    "UPDATE project_signing_identities SET content_hash = ?1 WHERE id = ?2",
                )
                .bind(content_hash(&material, key))
                .bind(&id)
                .execute(pool)
                .await?;
            }
            Err(e) => warn!(identity_id = %id, error = %e, "could not hash a signing identity"),
        }
    }
    Ok(())
}

struct Holder {
    project_id: String,
    pipeline_name: String,
    actor: Option<String>,
}

/// The identity holding `material` in the project, creating it when the
/// library has none. Returns its id and whether it was created.
async fn identity_for(
    pool: &SqlitePool,
    key: &[u8],
    holder: &Holder,
    kind: SigningIdentityKind,
    name_suffix: &str,
    material: &Material,
    summary: &Summary,
) -> anyhow::Result<(String, bool)> {
    let hash = content_hash(material, key);
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT id FROM project_signing_identities \
         WHERE project_id = ?1 AND kind = ?2 AND content_hash = ?3 \
         ORDER BY created_at, id LIMIT 1",
    )
    .bind(&holder.project_id)
    .bind(kind_str(kind))
    .bind(&hash)
    .fetch_optional(pool)
    .await?;
    if let Some(id) = existing {
        return Ok((id, false));
    }
    let name = unique_name(
        pool,
        &holder.project_id,
        &identity_base_name(&holder.pipeline_name, name_suffix),
    )
    .await?;
    let id = Uuid::new_v4().to_string();
    let now = now_unix();
    let mut plain = serde_json::to_string(material)?;
    let encrypted = crypto::encrypt(&plain, key);
    plain.zeroize();
    sqlx::query(
        "INSERT INTO project_signing_identities \
         (id, project_id, name, kind, summary, material_encrypted, content_hash, \
          created_by, updated_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?9)",
    )
    .bind(&id)
    .bind(&holder.project_id)
    .bind(&name)
    .bind(kind_str(kind))
    .bind(serde_json::to_string(summary)?)
    .bind(encrypted?)
    .bind(&hash)
    .bind(&holder.actor)
    .bind(now)
    .execute(pool)
    .await?;
    Ok((id, true))
}

/// Adopt the enabled pipeline signing that still uses its own files, for
/// one pipeline or (with `None`) every pipeline.
pub(crate) async fn adopt_pipeline_signing(
    pool: &SqlitePool,
    key: &[u8],
    pipeline_id: Option<&str>,
) -> anyhow::Result<AdoptionReport> {
    backfill_hashes(pool, key).await?;
    let mut report = AdoptionReport::default();
    adopt_android(pool, key, pipeline_id, &mut report).await?;
    adopt_ios(pool, key, pipeline_id, &mut report).await?;
    Ok(report)
}

async fn adopt_android(
    pool: &SqlitePool,
    key: &[u8],
    pipeline_id: Option<&str>,
    report: &mut AdoptionReport,
) -> anyhow::Result<()> {
    // Oldest pipeline first, release before debug, so a shared keystore is
    // named after the pipeline that had it first.
    let rows = sqlx::query(
        "SELECT s.id, s.build_type, s.keystore_filename, s.keystore_encrypted, \
                s.store_password_encrypted, s.key_alias_encrypted, s.key_password_encrypted, \
                s.updated_by, p.project_id, p.name AS pipeline_name \
         FROM pipeline_android_signing_profiles s JOIN pipelines p ON p.id = s.pipeline_id \
         WHERE s.enabled = 1 AND s.identity_id IS NULL AND s.keystore_encrypted IS NOT NULL \
           AND (?1 IS NULL OR s.pipeline_id = ?1) \
         ORDER BY p.created_at, p.id, s.build_type DESC",
    )
    .bind(pipeline_id)
    .fetch_all(pool)
    .await?;
    for row in rows {
        let row_id: String = row.get("id");
        let build_type: String = row.get("build_type");
        let material = (|| -> anyhow::Result<Material> {
            Ok(Material::AndroidKeystore {
                keystore_filename: row
                    .get::<Option<String>, _>("keystore_filename")
                    .unwrap_or_else(|| format!("{build_type}.jks")),
                keystore_base64: decrypt(row.get("keystore_encrypted"), key)?,
                store_password: decrypt(row.get("store_password_encrypted"), key)?,
                key_alias: decrypt(row.get("key_alias_encrypted"), key)?,
                key_password: decrypt(row.get("key_password_encrypted"), key)?,
            })
        })();
        let material = match material {
            Ok(material) => material,
            Err(e) => {
                warn!(profile_id = %row_id, error = %e, "could not move an Android keystore into the signing library");
                report.skipped += 1;
                continue;
            }
        };
        let Material::AndroidKeystore {
            keystore_filename,
            key_alias,
            ..
        } = &material
        else {
            unreachable!("built as android");
        };
        let summary = Summary {
            filename: Some(keystore_filename.clone()),
            key_alias: Some(key_alias.clone()),
            ..Summary::default()
        };
        let holder = Holder {
            project_id: row.get("project_id"),
            pipeline_name: row.get("pipeline_name"),
            actor: row.get("updated_by"),
        };
        let suffix = if build_type == "debug" {
            "debug-keystore"
        } else {
            "keystore"
        };
        let (identity_id, created) = identity_for(
            pool,
            key,
            &holder,
            SigningIdentityKind::AndroidKeystore,
            suffix,
            &material,
            &summary,
        )
        .await?;
        sqlx::query(
            "UPDATE pipeline_android_signing_profiles SET identity_id = ?1 \
             WHERE id = ?2 AND identity_id IS NULL",
        )
        .bind(&identity_id)
        .bind(&row_id)
        .execute(pool)
        .await?;
        report.created += usize::from(created);
        report.linked += 1;
    }
    Ok(())
}

async fn adopt_ios(
    pool: &SqlitePool,
    key: &[u8],
    pipeline_id: Option<&str>,
    report: &mut AdoptionReport,
) -> anyhow::Result<()> {
    let rows = sqlx::query(
        "SELECT s.id, s.pipeline_id, s.team_id, s.export_method, s.bundle_ids_json, \
                s.p12_filename, s.p12_encrypted, s.p12_password_encrypted, s.p12_expires_at, \
                s.updated_by, p.project_id, p.name AS pipeline_name \
         FROM pipeline_ios_signing_settings s JOIN pipelines p ON p.id = s.pipeline_id \
         WHERE s.enabled = 1 AND s.mode = 'manual' AND s.identity_id IS NULL \
           AND s.p12_encrypted IS NOT NULL AND s.team_id IS NOT NULL \
           AND (?1 IS NULL OR s.pipeline_id = ?1) \
         ORDER BY p.created_at, p.id",
    )
    .bind(pipeline_id)
    .fetch_all(pool)
    .await?;
    for row in rows {
        let settings_id: String = row.get("id");
        let row_pipeline: String = row.get("pipeline_id");
        let bundle_ids: Vec<String> =
            serde_json::from_str(&row.get::<String, _>("bundle_ids_json")).unwrap_or_default();
        let profile_rows = sqlx::query(
            "SELECT bundle_id, profile_filename, profile_encrypted, profile_uuid, profile_name, \
                    expires_at \
             FROM pipeline_ios_provisioning_profiles \
             WHERE pipeline_id = ?1 AND profile_encrypted IS NOT NULL",
        )
        .bind(&row_pipeline)
        .fetch_all(pool)
        .await?;
        let export_method = IosExportMethod::from_stored(&row.get::<String, _>("export_method"));
        let built = (|| -> anyhow::Result<(Material, Summary)> {
            let mut profiles = Vec::new();
            let mut summaries = Vec::new();
            // In the pipeline's bundle order, main app first.
            for bundle_id in &bundle_ids {
                let Some(profile) = profile_rows
                    .iter()
                    .find(|profile| &profile.get::<String, _>("bundle_id") == bundle_id)
                else {
                    continue;
                };
                let profile_uuid: Option<String> = profile.get("profile_uuid");
                let profile_name: Option<String> = profile.get("profile_name");
                profiles.push(RunnerIosProvisioningProfile {
                    bundle_id: bundle_id.clone(),
                    profile_filename: profile
                        .get::<Option<String>, _>("profile_filename")
                        .unwrap_or_else(|| format!("{bundle_id}.mobileprovision")),
                    profile_base64: decrypt(profile.get("profile_encrypted"), key)?,
                    profile_uuid: profile_uuid.clone(),
                    profile_name: profile_name.clone(),
                });
                summaries.push(SigningIdentityProfile {
                    bundle_id: bundle_id.clone(),
                    profile_name,
                    profile_uuid,
                    expires_at: profile.get("expires_at"),
                });
            }
            if profiles.is_empty() {
                anyhow::bail!("no provisioning profiles are stored");
            }
            let team_id: String = row.get("team_id");
            let p12_filename = row
                .get::<Option<String>, _>("p12_filename")
                .unwrap_or_else(|| "ios-distribution.p12".to_string());
            let summary = Summary {
                filename: Some(p12_filename.clone()),
                team_id: Some(team_id.clone()),
                export_method: Some(export_method),
                expires_at: row.get("p12_expires_at"),
                profiles: summaries,
                ..Summary::default()
            };
            Ok((
                Material::IosDistribution {
                    team_id,
                    export_method,
                    p12_filename,
                    p12_base64: decrypt(row.get("p12_encrypted"), key)?,
                    p12_password: decrypt(row.get("p12_password_encrypted"), key)?,
                    profiles,
                },
                summary,
            ))
        })();
        let (material, summary) = match built {
            Ok(built) => built,
            Err(e) => {
                warn!(pipeline_id = %row_pipeline, error = %e, "could not move an iOS certificate into the signing library");
                report.skipped += 1;
                continue;
            }
        };
        let holder = Holder {
            project_id: row.get("project_id"),
            pipeline_name: row.get("pipeline_name"),
            actor: row.get("updated_by"),
        };
        let (identity_id, created) = identity_for(
            pool,
            key,
            &holder,
            SigningIdentityKind::IosDistribution,
            "certificate",
            &material,
            &summary,
        )
        .await?;
        sqlx::query(
            "UPDATE pipeline_ios_signing_settings SET identity_id = ?1 \
             WHERE id = ?2 AND identity_id IS NULL",
        )
        .bind(&identity_id)
        .bind(&settings_id)
        .execute(pool)
        .await?;
        crate::signing_identities::sync_ios_pipelines(pool, &identity_id).await?;
        report.created += usize::from(created);
        report.linked += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use oore_contract::AndroidSigningBuildType;

    const KEY: [u8; 32] = [7u8; 32];

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
            "INSERT INTO projects (id, name, created_by, created_at, updated_at) VALUES \
             ('p1', 'Coin', 'u1', 0, 0), ('p2', 'Other', 'u1', 0, 0)",
            "INSERT INTO pipelines (id, project_id, name, trigger_config, created_at, updated_at) VALUES \
             ('pl1', 'p1', 'Coin Release', '{}', 1, 1), \
             ('pl2', 'p1', 'Coin Nightly', '{}', 2, 2), \
             ('pl3', 'p2', 'Other Release', '{}', 3, 3), \
             ('pl4', 'p2', 'Other Old', '{}', 4, 4)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        pool
    }

    fn enc(value: &str) -> String {
        crypto::encrypt(value, &KEY).unwrap()
    }

    async fn android_profile(
        pool: &SqlitePool,
        pipeline: &str,
        build_type: &str,
        enabled: bool,
        keystore: &str,
        alias: &str,
    ) {
        sqlx::query(
            "INSERT INTO pipeline_android_signing_profiles \
             (id, pipeline_id, build_type, enabled, keystore_filename, keystore_encrypted, \
              store_password_encrypted, key_alias_encrypted, key_password_encrypted, \
              created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, 0)",
        )
        .bind(format!("{pipeline}-{build_type}"))
        .bind(pipeline)
        .bind(build_type)
        .bind(i64::from(enabled))
        .bind(format!("{pipeline}.jks"))
        .bind(enc(keystore))
        .bind(enc("store-pw"))
        .bind(enc(alias))
        .bind(enc("key-pw"))
        .execute(pool)
        .await
        .unwrap();
    }

    async fn linked(pool: &SqlitePool, table: &str, id_column: &str, id: &str) -> Option<String> {
        let sql = match (table, id_column) {
            ("pipeline_android_signing_profiles", "id") => {
                "SELECT identity_id FROM pipeline_android_signing_profiles WHERE id = ?1"
            }
            _ => "SELECT identity_id FROM pipeline_ios_signing_settings WHERE id = ?1",
        };
        sqlx::query_scalar(sql)
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn identity_names(pool: &SqlitePool, project: &str) -> Vec<String> {
        sqlx::query_scalar(
            "SELECT name FROM project_signing_identities WHERE project_id = ?1 ORDER BY name",
        )
        .bind(project)
        .fetch_all(pool)
        .await
        .unwrap()
    }

    async fn job(pool: &SqlitePool, id: &str, pipeline: &str, project: &str, snapshot: &str) {
        sqlx::query(
            "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
             config_snapshot, queued_at, created_at, updated_at) \
             VALUES (?1, ?2, ?3, (SELECT COUNT(*) + 1 FROM builds), 'running', 'manual', ?4, 0, 0, 0)",
        )
        .bind(id)
        .bind(project)
        .bind(pipeline)
        .bind(snapshot)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn pipeline_files_move_into_the_library_and_builds_keep_their_keys() {
        let pool = pool().await;
        // The same release keystore on two pipelines of one app, a debug
        // keystore, and a disabled profile that stays where it is.
        android_profile(&pool, "pl1", "release", true, "S0VZLUE=", "upload").await;
        android_profile(&pool, "pl2", "release", true, "S0VZLUE=", "upload").await;
        android_profile(&pool, "pl1", "debug", true, "REVCVUc=", "androiddebugkey").await;
        android_profile(&pool, "pl4", "release", false, "T0xE", "old").await;
        // The other app already holds pl3's keystore, saved before hashes.
        let existing = Material::AndroidKeystore {
            keystore_filename: "library.jks".into(),
            keystore_base64: "S0VZLUE=".into(),
            store_password: "store-pw".into(),
            key_alias: "upload".into(),
            key_password: "key-pw".into(),
        };
        sqlx::query(
            "INSERT INTO project_signing_identities \
             (id, project_id, name, kind, summary, material_encrypted, created_at, updated_at) \
             VALUES ('lib-p2', 'p2', 'upload', 'android_keystore', '{}', ?1, 0, 0)",
        )
        .bind(enc(&serde_json::to_string(&existing).unwrap()))
        .execute(&pool)
        .await
        .unwrap();
        android_profile(&pool, "pl3", "release", true, "S0VZLUE=", "upload").await;
        // A manual iOS setup with one profile.
        sqlx::query(
            "INSERT INTO pipeline_ios_signing_settings \
             (id, pipeline_id, enabled, mode, team_id, export_method, bundle_ids_json, \
              p12_filename, p12_encrypted, p12_password_encrypted, p12_expires_at, \
              created_at, updated_at) \
             VALUES ('ios1', 'pl1', 1, 'manual', 'TEAM123456', 'app_store', '[\"com.coin\"]', \
                     'dist.p12', ?1, ?2, 999, 0, 0)",
        )
        .bind(enc("UDEy"))
        .bind(enc("p12-pw"))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO pipeline_ios_provisioning_profiles \
             (id, pipeline_id, bundle_id, profile_filename, profile_encrypted, profile_name, \
              expires_at, created_at, updated_at) \
             VALUES ('pp1', 'pl1', 'com.coin', 'coin.mobileprovision', ?1, 'Coin Store', 500, 0, 0)",
        )
        .bind(enc("UFJPRg=="))
        .execute(&pool)
        .await
        .unwrap();

        let report = adopt_pipeline_signing(&pool, &KEY, None).await.unwrap();
        assert_eq!(
            report,
            AdoptionReport {
                created: 3,
                linked: 5,
                skipped: 0
            }
        );
        assert_eq!(
            identity_names(&pool, "p1").await,
            [
                "coin-release-certificate",
                "coin-release-debug-keystore",
                "coin-release-keystore"
            ]
        );
        // One identity for the shared keystore; the other app reuses its own.
        let release = linked(
            &pool,
            "pipeline_android_signing_profiles",
            "id",
            "pl1-release",
        )
        .await;
        assert!(release.is_some());
        assert_eq!(
            linked(
                &pool,
                "pipeline_android_signing_profiles",
                "id",
                "pl2-release"
            )
            .await,
            release
        );
        assert_eq!(
            linked(
                &pool,
                "pipeline_android_signing_profiles",
                "id",
                "pl3-release"
            )
            .await
            .as_deref(),
            Some("lib-p2")
        );
        assert_eq!(identity_names(&pool, "p2").await, ["upload"]);
        assert_eq!(
            linked(
                &pool,
                "pipeline_android_signing_profiles",
                "id",
                "pl4-release"
            )
            .await,
            None
        );
        // The pipeline's own columns stay stored.
        let kept: Option<String> = sqlx::query_scalar(
            "SELECT keystore_encrypted FROM pipeline_android_signing_profiles WHERE id = 'pl1-release'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(kept.is_some());

        // Running again changes nothing.
        assert_eq!(
            adopt_pipeline_signing(&pool, &KEY, None).await.unwrap(),
            AdoptionReport::default()
        );

        // A build of pl2 signs with the same key, now from the library.
        job(&pool, "b1", "pl2", "p1", "{}").await;
        let signing = crate::pipeline_signing::job_android_signing(&pool, &KEY, "pl2", "b1")
            .await
            .unwrap();
        let profile = signing.release.unwrap();
        assert_eq!(profile.build_type, AndroidSigningBuildType::Release);
        assert_eq!(profile.keystore_base64, "S0VZLUE=");
        assert_eq!(profile.key_alias, "upload");
        assert_eq!(profile.store_password, "store-pw");
        assert!(signing.debug.is_none());
        job(&pool, "b2", "pl1", "p1", "{}").await;
        let signing = crate::pipeline_signing::job_android_signing(&pool, &KEY, "pl1", "b2")
            .await
            .unwrap();
        assert_eq!(signing.debug.unwrap().key_alias, "androiddebugkey");

        // A workflow's identity still wins over the pipeline's.
        sqlx::query(
            "INSERT INTO project_signing_identities \
             (id, project_id, name, kind, summary, material_encrypted, created_at, updated_at) \
             VALUES ('wf', 'p1', 'nightly', 'android_keystore', '{}', ?1, 0, 0)",
        )
        .bind(enc(&serde_json::to_string(&Material::AndroidKeystore {
            keystore_filename: "nightly.jks".into(),
            keystore_base64: "TklHSFQ=".into(),
            store_password: "s".into(),
            key_alias: "nightly".into(),
            key_password: "k".into(),
        })
        .unwrap()))
        .execute(&pool)
        .await
        .unwrap();
        job(
            &pool,
            "b3",
            "pl2",
            "p1",
            r#"{"signing_identities":{"android":"wf"}}"#,
        )
        .await;
        let signing = crate::pipeline_signing::job_android_signing(&pool, &KEY, "pl2", "b3")
            .await
            .unwrap();
        assert_eq!(signing.release.unwrap().key_alias, "nightly");

        // iOS: the certificate and profile became one identity, and the
        // pipeline follows its team, export method and bundles.
        let ios_identity = linked(&pool, "pipeline_ios_signing_settings", "id", "ios1")
            .await
            .unwrap();
        let summary: String =
            sqlx::query_scalar("SELECT summary FROM project_signing_identities WHERE id = ?1")
                .bind(&ios_identity)
                .fetch_one(&pool)
                .await
                .unwrap();
        let summary: serde_json::Value = serde_json::from_str(&summary).unwrap();
        assert_eq!(summary["team_id"], "TEAM123456");
        assert_eq!(summary["export_method"], "app_store");
        assert_eq!(summary["expires_at"], 999);
        assert_eq!(summary["profiles"][0]["bundle_id"], "com.coin");
        let bundle = crate::pipeline_ios_signing::job_ios_signing(&pool, &KEY, "pl1", "b2")
            .await
            .unwrap()
            .bundle
            .unwrap();
        assert_eq!(bundle.p12_base64, "UDEy");
        assert_eq!(bundle.p12_password, "p12-pw");
        assert_eq!(bundle.export_method, IosExportMethod::AppStore);
        assert_eq!(bundle.provisioning_profiles[0].profile_base64, "UFJPRg==");

        // A library identity a pipeline signs with cannot be deleted.
        assert!(
            sqlx::query("DELETE FROM project_signing_identities WHERE id = ?1")
                .bind(&ios_identity)
                .execute(&pool)
                .await
                .is_err()
        );
    }

    #[test]
    fn names_follow_the_pipeline() {
        assert_eq!(
            identity_base_name("Coin Release", "keystore"),
            "coin-release-keystore"
        );
        assert_eq!(
            identity_base_name("  ** iOS / Store **", "certificate"),
            "ios-store-certificate"
        );
        assert_eq!(
            identity_base_name("流水线", "keystore"),
            "pipeline-keystore"
        );
        let long = identity_base_name(&"a".repeat(80), "debug-keystore");
        assert!(
            long.len() <= 64 - 3 && is_valid_workflow_name(&long),
            "{long}"
        );
    }
}
