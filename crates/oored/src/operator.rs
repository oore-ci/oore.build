//! Host-authorized local operator protocol.
//!
//! The management socket is the only transport for these operations. The
//! daemon owns every database read and mutation in this module.

use anyhow::Context;
use base64::Engine;
use oore_contract::{
    ManagedRunnerCapabilities, ManagedRunnerRecord, ManagedRunnerRollback, OperatorRequest,
    OperatorRequestEnvelope, OperatorResponse, RUNNER_PROTOCOL_VERSION, SetupState,
};
use rand::RngCore;
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::token::{generate_token, hash_token};
use crate::util::now_unix;

const BARRIER_LEASE_SECS: i64 = 300;

pub async fn handle_local(
    pool: &SqlitePool,
    envelope: OperatorRequestEnvelope,
) -> anyhow::Result<OperatorResponse> {
    validate_instance_id(pool, &envelope.expected_instance_id).await?;
    match envelope.request {
        OperatorRequest::MintBootstrapToken { ttl_secs } => {
            mint_bootstrap_token(pool, ttl_secs).await
        }
        OperatorRequest::CreateFrontendInvite { ttl_secs } => {
            create_frontend_invite(pool, ttl_secs).await
        }
        OperatorRequest::EnsureManagedRunner {
            operation_id,
            name,
            capabilities,
            runner_id,
            runner_token,
            proposed_runner_id,
            proposed_runner_token,
            adopt_installed_registration,
        } => {
            ensure_managed_runner(
                pool,
                operation_id,
                name,
                capabilities,
                runner_id,
                runner_token,
                proposed_runner_id,
                proposed_runner_token,
                adopt_installed_registration,
            )
            .await
        }
        OperatorRequest::RunnerRegistrationMatches {
            runner_id,
            runner_token,
            allow_manual_for_adoption,
        } => registration_matches(pool, &runner_id, &runner_token, allow_manual_for_adoption).await,
        OperatorRequest::RunnerStatus { runner_id } => runner_status(pool, &runner_id).await,
        OperatorRequest::RestoreManagedRunner { rollback } => {
            restore_managed_runner(pool, rollback).await
        }
        OperatorRequest::RunnerBarrierAcquire => acquire_barrier(pool).await,
        OperatorRequest::RunnerBarrierRenew { lease_token } => {
            renew_barrier(pool, &lease_token).await
        }
        OperatorRequest::RunnerBarrierWait {
            lease_token,
            runner_id,
        } => {
            renew_barrier(pool, &lease_token).await?;
            wait_for_work(pool, runner_id.as_deref()).await
        }
        OperatorRequest::RunnerBarrierRelease { lease_token } => {
            release_barrier(pool, &lease_token).await
        }
    }
}

async fn validate_instance_id(pool: &SqlitePool, expected_instance_id: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        !expected_instance_id.trim().is_empty(),
        "expected instance identity is required"
    );
    let instance_id: String =
        sqlx::query_scalar("SELECT instance_id FROM setup_state WHERE id = 1")
            .fetch_one(pool)
            .await?;
    anyhow::ensure!(
        instance_id == expected_instance_id,
        "operator request targets a different daemon instance"
    );
    Ok(())
}

async fn mint_bootstrap_token(
    pool: &SqlitePool,
    ttl_secs: u64,
) -> anyhow::Result<OperatorResponse> {
    anyhow::ensure!(
        (1..=60 * 60).contains(&ttl_secs),
        "bootstrap token ttl must be between 1 second and 1 hour"
    );

    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
    let row = sqlx::query("SELECT setup_state, instance_id FROM setup_state WHERE id = 1")
        .fetch_one(&mut *transaction)
        .await?;
    let state = row
        .get::<&str, _>("setup_state")
        .parse::<SetupState>()
        .map_err(anyhow::Error::msg)?;
    anyhow::ensure!(state != SetupState::Ready, "setup is already complete");
    let instance_id: String = row.get("instance_id");
    let token = generate_token();
    let now = now_unix();
    let expires_at = now + i64::try_from(ttl_secs)?;
    let updated = sqlx::query(
        "UPDATE setup_state SET bootstrap_token_hash = ?1, \
         bootstrap_token_expires_at = ?2, bootstrap_token_consumed_at = NULL, \
         updated_at = ?3 WHERE id = 1 AND setup_state != 'ready'",
    )
    .bind(hash_token(&token))
    .bind(expires_at)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    anyhow::ensure!(updated.rows_affected() == 1, "setup state changed");
    transaction.commit().await?;

    Ok(OperatorResponse::BootstrapToken {
        token,
        expires_at,
        state,
        instance_id,
    })
}

async fn create_frontend_invite(
    pool: &SqlitePool,
    ttl_secs: u64,
) -> anyhow::Result<OperatorResponse> {
    anyhow::ensure!(
        (1..=60 * 60).contains(&ttl_secs),
        "frontend invite ttl must be between 1 second and 1 hour"
    );

    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
    let row = sqlx::query("SELECT setup_state, instance_id FROM setup_state WHERE id = 1")
        .fetch_one(&mut *transaction)
        .await?;
    let state: String = row.get("setup_state");
    anyhow::ensure!(
        state == SetupState::Ready.to_string(),
        "backend setup must be complete before frontend pairing"
    );
    let instance_id: String = row.get("instance_id");
    let configured: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM trusted_proxy_settings \
         WHERE id = 1 AND encrypted_shared_secret IS NOT NULL LIMIT 1",
    )
    .fetch_optional(&mut *transaction)
    .await?;
    anyhow::ensure!(
        configured.is_some(),
        "frontend pairing requires a configured trusted proxy backend"
    );

    let mut bytes = [0u8; 24];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let code = format!(
        "fp_{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    );
    let now = now_unix();
    let expires_at = now + i64::try_from(ttl_secs)?;
    let invite_id = Uuid::new_v4().to_string();
    sqlx::query("UPDATE frontend_pairing_invites SET consumed_at = ?1 WHERE consumed_at IS NULL")
        .bind(now)
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        "INSERT INTO frontend_pairing_invites \
         (id, token_hash, expires_at, consumed_at, created_at) \
         VALUES (?1, ?2, ?3, NULL, ?4)",
    )
    .bind(&invite_id)
    .bind(hash_token(&code))
    .bind(expires_at)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    let details =
        serde_json::json!({ "source": "local_cli", "expires_at": expires_at }).to_string();
    sqlx::query(
        "INSERT INTO audit_logs \
         (actor_id, action, resource_type, resource_id, details, created_at) \
         VALUES (NULL, 'frontend_pairing_invite_created', \
                 'frontend_pairing_invite', ?1, ?2, ?3)",
    )
    .bind(&invite_id)
    .bind(details)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;

    Ok(OperatorResponse::FrontendInvite {
        code,
        expires_at,
        instance_id,
    })
}

// This helper mirrors the wire request fields to keep protocol handling explicit.
#[allow(clippy::too_many_arguments)]
async fn ensure_managed_runner(
    pool: &SqlitePool,
    operation_id: String,
    name: String,
    capabilities: ManagedRunnerCapabilities,
    existing_id: Option<String>,
    existing_token: Option<String>,
    proposed_runner_id: String,
    proposed_runner_token: String,
    adopt_installed_registration: bool,
) -> anyhow::Result<OperatorResponse> {
    anyhow::ensure!(
        Uuid::parse_str(&operation_id).is_ok(),
        "managed runner operation identity is invalid"
    );
    anyhow::ensure!(
        Uuid::parse_str(&proposed_runner_id).is_ok(),
        "proposed managed runner identity is invalid"
    );
    validate_plaintext_token(&proposed_runner_token)?;
    validate_runner_capabilities(&capabilities)?;
    let name = name.trim();
    anyhow::ensure!(
        !name.is_empty() && name.len() <= 255,
        "runner name must be between 1 and 255 characters"
    );
    let capabilities_json = serde_json::to_string(&capabilities)?;
    let request_hash = managed_runner_request_hash(
        &operation_id,
        name,
        &capabilities_json,
        existing_id.as_deref(),
        existing_token.as_deref(),
        &proposed_runner_id,
        &proposed_runner_token,
        adopt_installed_registration,
    )?;
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
    let instance_id = load_instance_id(&mut transaction).await?;
    create_managed_runner_receipts_table(&mut transaction).await?;
    if let Some(response) = load_managed_runner_receipt(
        &mut transaction,
        &operation_id,
        &request_hash,
        existing_token.as_deref(),
        &proposed_runner_token,
        &instance_id,
    )
    .await?
    {
        transaction.commit().await?;
        return Ok(response);
    }

    let existing_row = if let Some(id) = existing_id.as_deref() {
        load_runner(&mut transaction, id).await?
    } else {
        None
    };

    if let (Some(token), Some(previous)) = (existing_token.as_deref(), existing_row.as_ref()) {
        if previous.registered_by.is_some() && !adopt_installed_registration {
            anyhow::bail!("the existing runner registration belongs to an operator");
        }
        if hash_token(token) == previous.token_hash {
            let changed = sqlx::query(
                "UPDATE runners SET name = ?1, capabilities = ?2, registered_by = NULL, \
                 updated_at = ?3 WHERE id = ?4 AND token_hash = ?5 AND \
                 (registered_by IS NULL OR ?6 = 1)",
            )
            .bind(name)
            .bind(&capabilities_json)
            .bind(now_unix())
            .bind(&previous.id)
            .bind(&previous.token_hash)
            .bind(i64::from(adopt_installed_registration))
            .execute(&mut *transaction)
            .await?;
            anyhow::ensure!(changed.rows_affected() == 1, "managed runner changed");
            let rollback = Some(ManagedRunnerRollback {
                runner_id: previous.id.clone(),
                issued_token_hash: previous.token_hash.clone(),
                previous: Some(previous.clone()),
            });
            let runner_id = previous.id.clone();
            store_managed_runner_receipt(
                &mut transaction,
                &operation_id,
                &request_hash,
                &runner_id,
                name,
                &previous.token_hash,
                false,
                &rollback,
            )
            .await?;
            transaction.commit().await?;
            return Ok(OperatorResponse::ManagedRunner {
                runner_id,
                runner_name: name.to_string(),
                runner_token: token.to_string(),
                enrolled: false,
                rollback,
                instance_id,
            });
        }
        if previous.registered_by.is_some() || adopt_installed_registration {
            anyhow::bail!("the installed runner config does not match its backend registration");
        }
    }

    let managed_id = match existing_row.as_ref() {
        Some(row) if row.registered_by.is_none() => Some(row.id.clone()),
        _ => {
            let named = sqlx::query_scalar::<_, String>(
                "SELECT id FROM runners \
                 WHERE name = ?1 AND registered_by IS NULL LIMIT 1",
            )
            .bind(name)
            .fetch_optional(&mut *transaction)
            .await?;
            if named.is_some() {
                named
            } else {
                let legacy = sqlx::query_scalar::<_, String>(
                    "SELECT id FROM runners WHERE registered_by IS NULL \
                     ORDER BY created_at LIMIT 2",
                )
                .fetch_all(&mut *transaction)
                .await?;
                (legacy.len() == 1).then(|| legacy[0].clone())
            }
        }
    };

    let issued_token_hash = hash_token(&proposed_runner_token);
    let now = now_unix();
    let (runner_id, previous) = if let Some(id) = managed_id {
        let previous = load_runner(&mut transaction, &id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("managed runner disappeared"))?;
        anyhow::ensure!(
            previous.registered_by.is_none(),
            "the managed runner changed ownership"
        );
        let changed = sqlx::query(
            "UPDATE runners SET name = ?1, token_hash = ?2, status = 'offline', \
             capabilities = ?3, last_heartbeat_at = NULL, updated_at = ?4 \
             WHERE id = ?5 AND registered_by IS NULL AND token_hash = ?6",
        )
        .bind(name)
        .bind(&issued_token_hash)
        .bind(&capabilities_json)
        .bind(now)
        .bind(&id)
        .bind(&previous.token_hash)
        .execute(&mut *transaction)
        .await?;
        anyhow::ensure!(changed.rows_affected() == 1, "managed runner changed");
        (id, Some(previous))
    } else {
        let id = proposed_runner_id;
        sqlx::query(
            "INSERT INTO runners \
             (id, name, token_hash, status, capabilities, registered_by, created_at, updated_at) \
             VALUES (?1, ?2, ?3, 'offline', ?4, NULL, ?5, ?5)",
        )
        .bind(&id)
        .bind(name)
        .bind(&issued_token_hash)
        .bind(&capabilities_json)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        (id, None)
    };
    let rollback = ManagedRunnerRollback {
        runner_id: runner_id.clone(),
        issued_token_hash: issued_token_hash.clone(),
        previous,
    };
    store_managed_runner_receipt(
        &mut transaction,
        &operation_id,
        &request_hash,
        &runner_id,
        name,
        &issued_token_hash,
        true,
        &Some(rollback.clone()),
    )
    .await?;
    transaction.commit().await?;

    Ok(OperatorResponse::ManagedRunner {
        runner_id,
        runner_name: name.to_string(),
        runner_token: proposed_runner_token,
        enrolled: true,
        rollback: Some(rollback),
        instance_id,
    })
}

#[derive(serde::Serialize)]
struct ManagedRunnerRequestFingerprint {
    operation_id: String,
    name: String,
    capabilities: String,
    existing_id: Option<String>,
    existing_token_hash: Option<String>,
    proposed_runner_id: String,
    proposed_runner_token_hash: String,
    adopt_installed_registration: bool,
}

#[allow(clippy::too_many_arguments)]
fn managed_runner_request_hash(
    operation_id: &str,
    name: &str,
    capabilities: &str,
    existing_id: Option<&str>,
    existing_token: Option<&str>,
    proposed_runner_id: &str,
    proposed_runner_token: &str,
    adopt_installed_registration: bool,
) -> anyhow::Result<String> {
    let fingerprint = ManagedRunnerRequestFingerprint {
        operation_id: operation_id.to_string(),
        name: name.to_string(),
        capabilities: capabilities.to_string(),
        existing_id: existing_id.map(str::to_string),
        existing_token_hash: existing_token.map(hash_token),
        proposed_runner_id: proposed_runner_id.to_string(),
        proposed_runner_token_hash: hash_token(proposed_runner_token),
        adopt_installed_registration,
    };
    Ok(hash_token(&serde_json::to_string(&fingerprint)?))
}

async fn create_managed_runner_receipts_table(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS managed_runner_operator_receipts (\
           operation_id TEXT PRIMARY KEY, \
           request_hash TEXT NOT NULL, \
           runner_id TEXT NOT NULL, \
           runner_name TEXT NOT NULL, \
           issued_token_hash TEXT NOT NULL, \
           enrolled INTEGER NOT NULL CHECK (enrolled IN (0, 1)), \
           rollback_json TEXT NOT NULL, \
           created_at INTEGER NOT NULL\
         )",
    )
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn load_managed_runner_receipt(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    operation_id: &str,
    request_hash: &str,
    existing_token: Option<&str>,
    proposed_runner_token: &str,
    instance_id: &str,
) -> anyhow::Result<Option<OperatorResponse>> {
    let Some(row) = sqlx::query(
        "SELECT request_hash, runner_id, runner_name, issued_token_hash, \
                enrolled, rollback_json \
         FROM managed_runner_operator_receipts WHERE operation_id = ?1",
    )
    .bind(operation_id)
    .fetch_optional(&mut **transaction)
    .await?
    else {
        return Ok(None);
    };

    let stored_request_hash: String = row.get("request_hash");
    anyhow::ensure!(
        stored_request_hash == request_hash,
        "managed runner operation identity was reused with different input"
    );
    let runner_id: String = row.get("runner_id");
    let runner_name: String = row.get("runner_name");
    let issued_token_hash: String = row.get("issued_token_hash");
    validate_token_hash(&issued_token_hash)?;
    let response_token = existing_token
        .filter(|token| hash_token(token) == issued_token_hash)
        .or_else(|| {
            (hash_token(proposed_runner_token) == issued_token_hash)
                .then_some(proposed_runner_token)
        })
        .context("managed runner retry does not contain the issued credential")?;
    let current = load_runner(transaction, &runner_id)
        .await?
        .context("managed runner changed after its operator receipt was committed")?;
    anyhow::ensure!(
        current.registered_by.is_none() && current.token_hash == issued_token_hash,
        "managed runner changed after its operator receipt was committed"
    );

    let rollback: Option<ManagedRunnerRollback> =
        serde_json::from_str(row.get::<String, _>("rollback_json").as_str())?;
    if let Some(rollback) = rollback.as_ref() {
        anyhow::ensure!(
            rollback.runner_id == runner_id && rollback.issued_token_hash == issued_token_hash,
            "managed runner receipt rollback proof is invalid"
        );
        if let Some(previous) = rollback.previous.as_ref() {
            validate_rollback_record(previous, &runner_id)?;
        }
    }
    let enrolled: i64 = row.get("enrolled");
    anyhow::ensure!(
        matches!(enrolled, 0 | 1),
        "managed runner receipt is invalid"
    );
    Ok(Some(OperatorResponse::ManagedRunner {
        runner_id,
        runner_name,
        runner_token: response_token.to_string(),
        enrolled: enrolled == 1,
        rollback,
        instance_id: instance_id.to_string(),
    }))
}

#[allow(clippy::too_many_arguments)]
async fn store_managed_runner_receipt(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    operation_id: &str,
    request_hash: &str,
    runner_id: &str,
    runner_name: &str,
    issued_token_hash: &str,
    enrolled: bool,
    rollback: &Option<ManagedRunnerRollback>,
) -> anyhow::Result<()> {
    let rollback_json = serde_json::to_string(rollback)?;
    sqlx::query(
        "INSERT INTO managed_runner_operator_receipts \
         (operation_id, request_hash, runner_id, runner_name, issued_token_hash, \
          enrolled, rollback_json, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(operation_id)
    .bind(request_hash)
    .bind(runner_id)
    .bind(runner_name)
    .bind(issued_token_hash)
    .bind(i64::from(enrolled))
    .bind(rollback_json)
    .bind(now_unix())
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn validate_plaintext_token(value: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "proposed runner token is invalid"
    );
    Ok(())
}

fn validate_runner_capabilities(capabilities: &ManagedRunnerCapabilities) -> anyhow::Result<()> {
    anyhow::ensure!(
        capabilities.os == "macos",
        "managed local runner operating system is unsupported"
    );
    anyhow::ensure!(
        matches!(capabilities.arch.as_str(), "aarch64" | "x86_64"),
        "managed local runner architecture is unsupported"
    );
    anyhow::ensure!(
        !capabilities.os_version.trim().is_empty()
            && !capabilities.version.trim().is_empty()
            && capabilities.protocol_version == RUNNER_PROTOCOL_VERSION,
        "managed local runner capability identity is incomplete"
    );
    Ok(())
}

async fn registration_matches(
    pool: &SqlitePool,
    runner_id: &str,
    token: &str,
    allow_manual_for_adoption: bool,
) -> anyhow::Result<OperatorResponse> {
    let mut transaction = pool.begin().await?;
    let instance_id = load_instance_id(&mut transaction).await?;
    let row = sqlx::query("SELECT token_hash, registered_by FROM runners WHERE id = ?1")
        .bind(runner_id)
        .fetch_optional(&mut *transaction)
        .await?;
    let matches = row.is_some_and(|row| {
        let registered_by: Option<String> = row.get("registered_by");
        let expected: String = row.get("token_hash");
        (registered_by.is_none() || allow_manual_for_adoption) && hash_token(token) == expected
    });
    transaction.commit().await?;
    Ok(OperatorResponse::RegistrationMatch {
        matches,
        instance_id,
    })
}

async fn runner_status(pool: &SqlitePool, runner_id: &str) -> anyhow::Result<OperatorResponse> {
    let mut transaction = pool.begin().await?;
    let instance_id = load_instance_id(&mut transaction).await?;
    let row =
        sqlx::query("SELECT status, last_heartbeat_at, capabilities FROM runners WHERE id = ?1")
            .bind(runner_id)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or_else(|| anyhow::anyhow!("runner not found"))?;
    let status = row.try_get("status")?;
    let last_heartbeat_at = row.try_get("last_heartbeat_at")?;
    let capabilities = row.try_get("capabilities")?;
    transaction.commit().await?;
    Ok(OperatorResponse::RunnerStatus {
        status,
        last_heartbeat_at,
        capabilities,
        instance_id,
    })
}

async fn restore_managed_runner(
    pool: &SqlitePool,
    rollback: ManagedRunnerRollback,
) -> anyhow::Result<OperatorResponse> {
    validate_token_hash(&rollback.issued_token_hash)?;
    if let Some(previous) = rollback.previous.as_ref() {
        validate_rollback_record(previous, &rollback.runner_id)?;
    }

    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
    let instance_id = load_instance_id(&mut transaction).await?;
    let current = sqlx::query("SELECT token_hash, registered_by FROM runners WHERE id = ?1")
        .bind(&rollback.runner_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| anyhow::anyhow!("managed runner no longer exists"))?;
    let current_registered_by: Option<String> = current.get("registered_by");
    let current_token_hash: String = current.get("token_hash");
    anyhow::ensure!(
        current_registered_by.is_none(),
        "managed runner changed ownership"
    );
    anyhow::ensure!(
        current_token_hash == rollback.issued_token_hash,
        "managed runner credentials changed"
    );

    let changed = if let Some(previous) = rollback.previous {
        sqlx::query(
            "UPDATE runners SET name = ?1, token_hash = ?2, status = ?3, \
             capabilities = ?4, last_heartbeat_at = ?5, registered_by = ?6, \
             created_at = ?7, updated_at = ?8 \
             WHERE id = ?9 AND registered_by IS NULL AND token_hash = ?10",
        )
        .bind(previous.name)
        .bind(previous.token_hash)
        .bind(previous.status)
        .bind(previous.capabilities)
        .bind(previous.last_heartbeat_at)
        .bind(previous.registered_by)
        .bind(previous.created_at)
        .bind(previous.updated_at)
        .bind(&rollback.runner_id)
        .bind(&rollback.issued_token_hash)
        .execute(&mut *transaction)
        .await?
    } else {
        sqlx::query(
            "DELETE FROM runners \
             WHERE id = ?1 AND registered_by IS NULL AND token_hash = ?2",
        )
        .bind(&rollback.runner_id)
        .bind(&rollback.issued_token_hash)
        .execute(&mut *transaction)
        .await?
    };
    anyhow::ensure!(changed.rows_affected() == 1, "managed runner changed");
    transaction.commit().await?;

    Ok(OperatorResponse::ManagedRunnerRestored {
        runner_id: rollback.runner_id,
        instance_id,
    })
}

fn validate_rollback_record(record: &ManagedRunnerRecord, runner_id: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        record.id == runner_id,
        "rollback runner identity is invalid"
    );
    anyhow::ensure!(
        record
            .registered_by
            .as_deref()
            .is_none_or(|owner| !owner.trim().is_empty()),
        "rollback runner owner is invalid"
    );
    anyhow::ensure!(
        !record.name.trim().is_empty() && record.name.len() <= 255,
        "rollback runner name is invalid"
    );
    anyhow::ensure!(
        matches!(
            record.status.as_str(),
            "online" | "offline" | "busy" | "draining"
        ),
        "rollback runner status is invalid"
    );
    validate_token_hash(&record.token_hash)?;
    let capabilities: serde_json::Value = serde_json::from_str(&record.capabilities)?;
    anyhow::ensure!(
        capabilities.is_object(),
        "rollback runner capabilities are invalid"
    );
    Ok(())
}

fn validate_token_hash(value: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "runner token hash is invalid"
    );
    Ok(())
}

async fn acquire_barrier(pool: &SqlitePool) -> anyhow::Result<OperatorResponse> {
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
    let instance_id = load_instance_id(&mut transaction).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS runner_service_transition_lease (\
           id INTEGER PRIMARY KEY CHECK (id = 1), \
           token TEXT NOT NULL, \
           expires_at INTEGER NOT NULL\
         )",
    )
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "CREATE TRIGGER IF NOT EXISTS block_runner_claim_during_service_transition \
         BEFORE UPDATE OF status ON builds \
         WHEN OLD.status = 'queued' AND NEW.status = 'scheduled' \
          AND EXISTS (\
            SELECT 1 FROM runner_service_transition_lease \
            WHERE id = 1 AND expires_at >= CAST(strftime('%s', 'now') AS INTEGER)\
          ) \
         BEGIN \
           SELECT RAISE(ABORT, 'runner service transition in progress'); \
         END",
    )
    .execute(&mut *transaction)
    .await?;
    let now = now_unix();
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM runner_service_transition_lease \
         WHERE id = 1 AND expires_at >= ?1)",
    )
    .bind(now)
    .fetch_one(&mut *transaction)
    .await?;
    anyhow::ensure!(
        !active,
        "another runner service transition is already in progress"
    );
    let lease_token = generate_token();
    sqlx::query(
        "INSERT INTO runner_service_transition_lease (id, token, expires_at) \
         VALUES (1, ?1, ?2) ON CONFLICT(id) DO UPDATE SET \
         token = excluded.token, expires_at = excluded.expires_at",
    )
    .bind(&lease_token)
    .bind(now + BARRIER_LEASE_SECS)
    .execute(&mut *transaction)
    .await?;

    let build_events_exist: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master \
         WHERE type = 'table' AND name = 'build_events')",
    )
    .fetch_one(&mut *transaction)
    .await?;
    if build_events_exist {
        sqlx::query(
            "INSERT INTO build_events \
             (id, build_id, from_status, to_status, actor, reason, created_at) \
             SELECT lower(hex(randomblob(16))), id, 'scheduled', 'queued', 'system', \
                    'Requeued before a maintenance drain because no runner had accepted the build', ?1 \
             FROM builds WHERE status = 'scheduled' AND runner_id IS NULL",
        )
        .bind(now)
        .execute(&mut *transaction)
        .await?;
    }
    sqlx::query(
        "UPDATE builds SET status = 'queued', updated_at = ?1 \
         WHERE status = 'scheduled' AND runner_id IS NULL",
    )
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;

    Ok(OperatorResponse::Barrier {
        lease_token,
        instance_id,
    })
}

async fn renew_barrier(pool: &SqlitePool, token: &str) -> anyhow::Result<OperatorResponse> {
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
    let instance_id = load_instance_id(&mut transaction).await?;
    let now = now_unix();
    let changed = sqlx::query(
        "UPDATE runner_service_transition_lease SET expires_at = ?1 \
         WHERE id = 1 AND token = ?2 AND expires_at >= ?3",
    )
    .bind(now + BARRIER_LEASE_SECS)
    .bind(token)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    anyhow::ensure!(
        changed.rows_affected() == 1,
        "runner service transition lease was lost"
    );
    transaction.commit().await?;
    Ok(OperatorResponse::Barrier {
        lease_token: token.to_string(),
        instance_id,
    })
}

async fn wait_for_work(
    pool: &SqlitePool,
    runner_id: Option<&str>,
) -> anyhow::Result<OperatorResponse> {
    let mut transaction = pool.begin().await?;
    let instance_id = load_instance_id(&mut transaction).await?;
    let active: bool = if let Some(id) = runner_id {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM builds \
             WHERE (runner_id = ?1 AND status IN ('assigned', 'running')) \
                OR (runner_id IS NULL AND status = 'assigned'))",
        )
        .bind(id)
        .fetch_one(&mut *transaction)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM builds WHERE status IN ('assigned', 'running'))",
        )
        .fetch_one(&mut *transaction)
        .await?
    };
    transaction.commit().await?;
    Ok(OperatorResponse::BarrierWait {
        active,
        instance_id,
    })
}

async fn release_barrier(pool: &SqlitePool, token: &str) -> anyhow::Result<OperatorResponse> {
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
    let instance_id = load_instance_id(&mut transaction).await?;
    let changed =
        sqlx::query("DELETE FROM runner_service_transition_lease WHERE id = 1 AND token = ?1")
            .bind(token)
            .execute(&mut *transaction)
            .await?;
    anyhow::ensure!(
        changed.rows_affected() == 1,
        "runner service transition lease was lost"
    );
    transaction.commit().await?;
    Ok(OperatorResponse::Barrier {
        lease_token: token.to_string(),
        instance_id,
    })
}

async fn load_instance_id(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
) -> anyhow::Result<String> {
    Ok(
        sqlx::query_scalar("SELECT instance_id FROM setup_state WHERE id = 1")
            .fetch_one(&mut **transaction)
            .await?,
    )
}

async fn load_runner(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    runner_id: &str,
) -> anyhow::Result<Option<ManagedRunnerRecord>> {
    let row = sqlx::query(
        "SELECT id, name, token_hash, status, capabilities, last_heartbeat_at, \
         registered_by, created_at, updated_at FROM runners WHERE id = ?1",
    )
    .bind(runner_id)
    .fetch_optional(&mut **transaction)
    .await?;
    row.map(row_to_managed_runner).transpose()
}

fn row_to_managed_runner(row: SqliteRow) -> anyhow::Result<ManagedRunnerRecord> {
    Ok(ManagedRunnerRecord {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        token_hash: row.try_get("token_hash")?,
        status: row.try_get("status")?,
        capabilities: row.try_get("capabilities")?,
        last_heartbeat_at: row.try_get("last_heartbeat_at")?,
        registered_by: row.try_get("registered_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
