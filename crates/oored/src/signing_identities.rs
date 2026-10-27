//! The project credential library: named signing identities that v2 files
//! and per-workflow pipeline settings pick by name, so each workflow signs
//! with its own keys instead of the pipeline's.
//!
//! Resolution happens when a build is created and is recorded in its
//! snapshot (`signing_identities`, `publish_disabled`), so the runner's
//! signing and publish lookups need no file read and keep their shape.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use base64::Engine;
use oore_contract::{
    AndroidKeystoreInput, AndroidSigningBuildType, ApiError, CreateSigningIdentityRequest,
    IosDistributionInput, IosExportMethod, IosSigningMode, ListPipelineWorkflowSettingsResponse,
    ListSigningIdentitiesResponse, OkResponse, PipelineWorkflowSettings,
    RunnerAndroidSigningProfile, RunnerIosProvisioningProfile, RunnerIosSigningBundle,
    SigningIdentity, SigningIdentityKind, SigningIdentityProfile, SigningIdentityResponse,
    SigningIdentityUse, SigningSpec, UpdatePipelineWorkflowSettingsRequest,
    UpdateSigningIdentityRequest, is_valid_workflow_name,
};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use tracing::error;
use uuid::Uuid;
use zeroize::Zeroize;

use crate::AppState;
use crate::crypto;
use crate::extractors::AuthUser;
use crate::project_rbac::{
    ProjectPermission, require_pipeline_project_permission, require_project_permission,
    resolve_effective_project_role,
};
use crate::util::{api_err, now_unix};

type ApiFailure = (StatusCode, Json<ApiError>);
type ApiResult<T> = Result<Json<T>, ApiFailure>;

const MAX_NAME_LEN: usize = 64;
const MAX_KEYSTORE_BYTES: usize = 1024 * 1024;
const MAX_P12_BYTES: usize = 256 * 1024;
const MAX_PROFILE_BYTES: usize = 256 * 1024;
const MAX_PROFILES: usize = 32;

fn store_error(context: &'static str, e: impl std::fmt::Display) -> ApiFailure {
    error!(error = %e, "{context}");
    api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", context)
}

fn invalid(message: impl Into<String>) -> ApiFailure {
    api_err(StatusCode::BAD_REQUEST, "invalid_input", message)
}

fn kind_str(kind: SigningIdentityKind) -> &'static str {
    match kind {
        SigningIdentityKind::AndroidKeystore => "android_keystore",
        SigningIdentityKind::IosDistribution => "ios_distribution",
    }
}

fn parse_kind(raw: &str) -> SigningIdentityKind {
    match raw {
        "ios_distribution" => SigningIdentityKind::IosDistribution,
        _ => SigningIdentityKind::AndroidKeystore,
    }
}

/// Names follow workflow names, so a file can say `signing: { ios: coin-store }`.
fn normalize_name(name: &str) -> Result<String, ApiFailure> {
    let name = name.trim();
    if !is_valid_workflow_name(name) || name.len() > MAX_NAME_LEN {
        return Err(invalid(
            "name must start with a letter or digit and use only letters, digits, '-' and '_' (max 64 chars)",
        ));
    }
    Ok(name.to_string())
}

fn decode(value: &str, field: &str, max: usize) -> Result<Vec<u8>, ApiFailure> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value.trim())
        .map_err(|_| invalid(format!("{field} must be base64")))?;
    if bytes.is_empty() {
        return Err(invalid(format!("{field} must not be empty")));
    }
    if bytes.len() > max {
        return Err(invalid(format!("{field} is larger than {max} bytes")));
    }
    Ok(bytes)
}

fn required(value: &str, field: &str) -> Result<String, ApiFailure> {
    let value = value.trim();
    if value.is_empty() {
        return Err(invalid(format!("{field} is required")));
    }
    Ok(value.to_string())
}

// ── Stored shapes ───────────────────────────────────────────────

/// Encrypted as a whole into `material_encrypted`.
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Material {
    AndroidKeystore {
        keystore_filename: String,
        keystore_base64: String,
        store_password: String,
        key_alias: String,
        key_password: String,
    },
    IosDistribution {
        team_id: String,
        export_method: IosExportMethod,
        p12_filename: String,
        p12_base64: String,
        p12_password: String,
        profiles: Vec<RunnerIosProvisioningProfile>,
    },
}

impl Drop for Material {
    fn drop(&mut self) {
        match self {
            Material::AndroidKeystore {
                keystore_base64,
                store_password,
                key_password,
                ..
            } => {
                keystore_base64.zeroize();
                store_password.zeroize();
                key_password.zeroize();
            }
            Material::IosDistribution {
                p12_base64,
                p12_password,
                profiles,
                ..
            } => {
                p12_base64.zeroize();
                p12_password.zeroize();
                for profile in profiles {
                    profile.profile_base64.zeroize();
                }
            }
        }
    }
}

/// What the API shows, stored in `summary`.
#[derive(Default, Serialize, Deserialize)]
struct Summary {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    key_alias: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    team_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    export_method: Option<IosExportMethod>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expires_at: Option<i64>,
    #[serde(default)]
    profiles: Vec<SigningIdentityProfile>,
}

fn android_material(input: &AndroidKeystoreInput) -> Result<(Material, Summary), ApiFailure> {
    decode(
        &input.keystore_base64,
        "android.keystore_base64",
        MAX_KEYSTORE_BYTES,
    )?;
    let material = Material::AndroidKeystore {
        keystore_filename: required(&input.keystore_filename, "android.keystore_filename")?,
        keystore_base64: input.keystore_base64.trim().to_string(),
        store_password: required(&input.store_password, "android.store_password")?,
        key_alias: required(&input.key_alias, "android.key_alias")?,
        key_password: required(&input.key_password, "android.key_password")?,
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
    Ok((material, summary))
}

async fn ios_material(input: &IosDistributionInput) -> Result<(Material, Summary), ApiFailure> {
    let p12 = decode(&input.p12_base64, "ios.p12_base64", MAX_P12_BYTES)?;
    let p12_filename = required(&input.p12_filename, "ios.p12_filename")?;
    if input.profiles.is_empty() {
        return Err(invalid(
            "ios.profiles needs at least one provisioning profile",
        ));
    }
    if input.profiles.len() > MAX_PROFILES {
        return Err(invalid(format!(
            "ios.profiles holds at most {MAX_PROFILES} profiles"
        )));
    }
    let expires_at = crate::pipeline_ios_signing::inspect_p12(p12, input.p12_password.clone())
        .await
        .map_err(|e| {
            invalid(format!(
                "The certificate could not be opened with that password: {e:#}"
            ))
        })?;

    let mut team_id: Option<String> = None;
    let mut export_method: Option<IosExportMethod> = None;
    let mut profiles = Vec::new();
    let mut summaries = Vec::new();
    for (index, profile) in input.profiles.iter().enumerate() {
        let field = format!("ios.profiles[{index}]");
        let bytes = decode(
            &profile.profile_base64,
            &format!("{field}.profile_base64"),
            MAX_PROFILE_BYTES,
        )?;
        let inspected = crate::pipeline_ios_signing::inspect_provisioning_profile(bytes)
            .await
            .map_err(|e| invalid(format!("{field} is not a provisioning profile: {e:#}")))?;
        let Some(method) = inspected.export_method else {
            return Err(invalid(format!(
                "{field} is a {} profile; use an ad hoc or App Store distribution profile",
                inspected.distribution
            )));
        };
        if export_method.is_some_and(|existing| existing != method) {
            return Err(invalid(
                "ios.profiles mixes ad hoc and App Store profiles; one identity exports one way",
            ));
        }
        export_method = Some(method);
        if let Some(profile_team) = &inspected.team_id {
            if team_id.as_ref().is_some_and(|team| team != profile_team) {
                return Err(invalid("ios.profiles belong to different Apple teams"));
            }
            team_id = Some(profile_team.clone());
        }
        if summaries
            .iter()
            .any(|existing: &SigningIdentityProfile| existing.bundle_id == inspected.bundle_id)
        {
            return Err(invalid(format!(
                "ios.profiles has two profiles for {}",
                inspected.bundle_id
            )));
        }
        profiles.push(RunnerIosProvisioningProfile {
            bundle_id: inspected.bundle_id.clone(),
            profile_filename: required(
                &profile.profile_filename,
                &format!("{field}.profile_filename"),
            )?,
            profile_base64: profile.profile_base64.trim().to_string(),
            profile_uuid: inspected.profile_uuid.clone(),
            profile_name: inspected.profile_name.clone(),
        });
        summaries.push(SigningIdentityProfile {
            bundle_id: inspected.bundle_id,
            profile_name: inspected.profile_name,
            profile_uuid: inspected.profile_uuid,
            expires_at: inspected.expires_at,
        });
    }
    let team_id = team_id.ok_or_else(|| invalid("ios.profiles do not name an Apple team"))?;
    let export_method = export_method.expect("at least one profile");
    let summary = Summary {
        filename: Some(p12_filename.clone()),
        team_id: Some(team_id.clone()),
        export_method: Some(export_method),
        expires_at,
        profiles: summaries,
        ..Summary::default()
    };
    Ok((
        Material::IosDistribution {
            team_id,
            export_method,
            p12_filename,
            p12_base64: input.p12_base64.trim().to_string(),
            p12_password: input.p12_password.clone(),
            profiles,
        },
        summary,
    ))
}

async fn material_for(
    kind: SigningIdentityKind,
    android: Option<&AndroidKeystoreInput>,
    ios: Option<&IosDistributionInput>,
) -> Result<(Material, Summary), ApiFailure> {
    match (kind, android, ios) {
        (SigningIdentityKind::AndroidKeystore, Some(input), None) => android_material(input),
        (SigningIdentityKind::IosDistribution, None, Some(input)) => ios_material(input).await,
        (SigningIdentityKind::AndroidKeystore, _, _) => Err(invalid(
            "an android_keystore identity takes `android` and no `ios`",
        )),
        (SigningIdentityKind::IosDistribution, _, _) => Err(invalid(
            "an ios_distribution identity takes `ios` and no `android`",
        )),
    }
}

// ── Reading ─────────────────────────────────────────────────────

async fn uses_of(
    pool: &SqlitePool,
    identity_id: &str,
) -> Result<Vec<SigningIdentityUse>, ApiFailure> {
    let rows = sqlx::query(
        "SELECT s.pipeline_id, p.name AS pipeline_name, s.workflow \
         FROM pipeline_workflow_settings s JOIN pipelines p ON p.id = s.pipeline_id \
         WHERE s.android_identity_id = ?1 OR s.ios_identity_id = ?1 \
         ORDER BY p.name, s.workflow",
    )
    .bind(identity_id)
    .fetch_all(pool)
    .await
    .map_err(|e| store_error("Failed to load where the identity is used", e))?;
    Ok(rows
        .iter()
        .map(|row| SigningIdentityUse {
            pipeline_id: row.get("pipeline_id"),
            pipeline_name: row.get("pipeline_name"),
            workflow: row.get("workflow"),
        })
        .collect())
}

async fn row_to_identity(
    pool: &SqlitePool,
    row: &sqlx::sqlite::SqliteRow,
) -> Result<SigningIdentity, ApiFailure> {
    let summary: Summary =
        serde_json::from_str(&row.get::<String, _>("summary")).unwrap_or_default();
    let id: String = row.get("id");
    Ok(SigningIdentity {
        used_by: uses_of(pool, &id).await?,
        id,
        project_id: row.get("project_id"),
        name: row.get("name"),
        kind: parse_kind(&row.get::<String, _>("kind")),
        filename: summary.filename,
        key_alias: summary.key_alias,
        team_id: summary.team_id,
        export_method: summary.export_method,
        expires_at: summary.expires_at,
        profiles: summary.profiles,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

async fn load_identity_row(
    pool: &SqlitePool,
    project_id: &str,
    identity_id: &str,
) -> Result<sqlx::sqlite::SqliteRow, ApiFailure> {
    sqlx::query("SELECT * FROM project_signing_identities WHERE id = ?1 AND project_id = ?2")
        .bind(identity_id)
        .bind(project_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| store_error("Failed to load the signing identity", e))?
        .ok_or_else(|| {
            api_err(
                StatusCode::NOT_FOUND,
                "not_found",
                "Signing identity not found",
            )
        })
}

async fn require_project(
    state: &AppState,
    auth: &AuthUser,
    project_id: &str,
    permission: ProjectPermission,
) -> Result<(), ApiFailure> {
    let effective = resolve_effective_project_role(
        &state.db,
        &auth.0.user_id,
        &auth.0.role,
        project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, permission)
}

fn name_taken(e: &sqlx::Error, name: &str) -> Option<ApiFailure> {
    e.as_database_error()
        .filter(|db| db.is_unique_violation())
        .map(|_| {
            api_err(
                StatusCode::CONFLICT,
                "name_taken",
                format!("This project already has a signing identity named {name}"),
            )
        })
}

// ── Handlers ────────────────────────────────────────────────────

/// `GET /v1/projects/{project_id}/signing-identities`
pub async fn list_signing_identities(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
) -> ApiResult<ListSigningIdentitiesResponse> {
    require_project(&state, &auth, &project_id, ProjectPermission::Read).await?;
    let rows = sqlx::query(
        "SELECT * FROM project_signing_identities WHERE project_id = ?1 ORDER BY kind, name",
    )
    .bind(&project_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| store_error("Failed to list signing identities", e))?;
    let mut identities = Vec::with_capacity(rows.len());
    for row in &rows {
        identities.push(row_to_identity(&state.db, row).await?);
    }
    Ok(Json(ListSigningIdentitiesResponse { identities }))
}

/// `POST /v1/projects/{project_id}/signing-identities`
pub async fn create_signing_identity(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
    Json(req): Json<CreateSigningIdentityRequest>,
) -> Result<(StatusCode, Json<SigningIdentityResponse>), ApiFailure> {
    require_project(
        &state,
        &auth,
        &project_id,
        ProjectPermission::ManagePipelines,
    )
    .await?;
    let name = normalize_name(&req.name)?;
    let (material, summary) =
        material_for(req.kind, req.android.as_ref(), req.ios.as_ref()).await?;
    let encrypted = crypto::encrypt(
        &serde_json::to_string(&material).expect("material serializes"),
        &state.encryption_key,
    )
    .map_err(|e| store_error("Failed to encrypt the signing identity", e))?;
    let id = Uuid::new_v4().to_string();
    let now = now_unix();
    sqlx::query(
        "INSERT INTO project_signing_identities \
         (id, project_id, name, kind, summary, material_encrypted, created_by, updated_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8, ?8)",
    )
    .bind(&id)
    .bind(&project_id)
    .bind(&name)
    .bind(kind_str(req.kind))
    .bind(serde_json::to_string(&summary).expect("summary serializes"))
    .bind(&encrypted)
    .bind(&auth.0.user_id)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| name_taken(&e, &name).unwrap_or_else(|| store_error("Failed to save the signing identity", e)))?;
    let row = load_identity_row(&state.db, &project_id, &id).await?;
    Ok((
        StatusCode::CREATED,
        Json(SigningIdentityResponse {
            identity: row_to_identity(&state.db, &row).await?,
        }),
    ))
}

/// `PATCH /v1/projects/{project_id}/signing-identities/{identity_id}`
pub async fn update_signing_identity(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((project_id, identity_id)): Path<(String, String)>,
    Json(req): Json<UpdateSigningIdentityRequest>,
) -> ApiResult<SigningIdentityResponse> {
    require_project(
        &state,
        &auth,
        &project_id,
        ProjectPermission::ManagePipelines,
    )
    .await?;
    let row = load_identity_row(&state.db, &project_id, &identity_id).await?;
    let kind = parse_kind(&row.get::<String, _>("kind"));
    let name = match &req.name {
        Some(name) => normalize_name(name)?,
        None => row.get("name"),
    };
    let replaced = if req.android.is_some() || req.ios.is_some() {
        let (material, summary) =
            material_for(kind, req.android.as_ref(), req.ios.as_ref()).await?;
        Some((
            crypto::encrypt(
                &serde_json::to_string(&material).expect("material serializes"),
                &state.encryption_key,
            )
            .map_err(|e| store_error("Failed to encrypt the signing identity", e))?,
            serde_json::to_string(&summary).expect("summary serializes"),
        ))
    } else {
        None
    };
    let (encrypted, summary) = replaced.unwrap_or_else(|| {
        (
            row.get("material_encrypted"),
            row.get::<String, _>("summary"),
        )
    });
    sqlx::query(
        "UPDATE project_signing_identities \
         SET name = ?1, summary = ?2, material_encrypted = ?3, updated_by = ?4, updated_at = ?5 \
         WHERE id = ?6",
    )
    .bind(&name)
    .bind(&summary)
    .bind(&encrypted)
    .bind(&auth.0.user_id)
    .bind(now_unix())
    .bind(&identity_id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        name_taken(&e, &name)
            .unwrap_or_else(|| store_error("Failed to save the signing identity", e))
    })?;
    let row = load_identity_row(&state.db, &project_id, &identity_id).await?;
    Ok(Json(SigningIdentityResponse {
        identity: row_to_identity(&state.db, &row).await?,
    }))
}

/// `DELETE /v1/projects/{project_id}/signing-identities/{identity_id}`.
/// Refused while a pipeline's workflow settings pick it.
pub async fn delete_signing_identity(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((project_id, identity_id)): Path<(String, String)>,
) -> ApiResult<OkResponse> {
    require_project(
        &state,
        &auth,
        &project_id,
        ProjectPermission::ManagePipelines,
    )
    .await?;
    load_identity_row(&state.db, &project_id, &identity_id).await?;
    let uses = uses_of(&state.db, &identity_id).await?;
    if !uses.is_empty() {
        let list = uses
            .iter()
            .map(|usage| format!("{} ({})", usage.pipeline_name, usage.workflow))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(api_err(
            StatusCode::CONFLICT,
            "identity_in_use",
            format!("Workflow settings use this identity: {list}. Pick another there first."),
        ));
    }
    sqlx::query("DELETE FROM project_signing_identities WHERE id = ?1")
        .bind(&identity_id)
        .execute(&state.db)
        .await
        .map_err(|e| store_error("Failed to delete the signing identity", e))?;
    Ok(Json(OkResponse { ok: true }))
}

// ── Per-workflow pipeline settings ──────────────────────────────

fn settings_from_row(row: &sqlx::sqlite::SqliteRow) -> PipelineWorkflowSettings {
    PipelineWorkflowSettings {
        workflow: row.get("workflow"),
        android_identity_id: row.get("android_identity_id"),
        ios_identity_id: row.get("ios_identity_id"),
        publish_enabled: row.get::<Option<i64>, _>("publish_enabled").map(|v| v != 0),
        approval_required: row
            .get::<Option<i64>, _>("approval_required")
            .map(|v| v != 0),
        updated_at: Some(row.get("updated_at")),
    }
}

pub(crate) async fn load_workflow_settings(
    pool: &SqlitePool,
    pipeline_id: &str,
    workflow: &str,
) -> Result<Option<PipelineWorkflowSettings>, sqlx::Error> {
    Ok(sqlx::query(
        "SELECT * FROM pipeline_workflow_settings WHERE pipeline_id = ?1 AND workflow = ?2",
    )
    .bind(pipeline_id)
    .bind(workflow)
    .fetch_optional(pool)
    .await?
    .as_ref()
    .map(settings_from_row))
}

/// `GET /v1/pipelines/{pipeline_id}/workflow-settings`
pub async fn list_pipeline_workflow_settings(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
) -> ApiResult<ListPipelineWorkflowSettingsResponse> {
    require_pipeline_project_permission(
        &state.db,
        &auth.0.user_id,
        &auth.0.role,
        &auth.0.auth_source,
        &pipeline_id,
        ProjectPermission::Read,
    )
    .await?;
    let rows = sqlx::query(
        "SELECT * FROM pipeline_workflow_settings WHERE pipeline_id = ?1 ORDER BY workflow",
    )
    .bind(&pipeline_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| store_error("Failed to load workflow settings", e))?;
    Ok(Json(ListPipelineWorkflowSettingsResponse {
        pipeline_id,
        settings: rows.iter().map(settings_from_row).collect(),
    }))
}

async fn check_identity(
    pool: &SqlitePool,
    project_id: &str,
    identity_id: Option<&str>,
    kind: SigningIdentityKind,
    field: &str,
) -> Result<(), ApiFailure> {
    let Some(identity_id) = identity_id else {
        return Ok(());
    };
    let stored: Option<String> = sqlx::query_scalar(
        "SELECT kind FROM project_signing_identities WHERE id = ?1 AND project_id = ?2",
    )
    .bind(identity_id)
    .bind(project_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| store_error("Failed to load the signing identity", e))?;
    match stored {
        None => Err(invalid(format!(
            "{field} is not a signing identity of this project"
        ))),
        Some(stored) if parse_kind(&stored) != kind => Err(invalid(format!(
            "{field} must be an {} identity",
            kind_str(kind)
        ))),
        Some(_) => Ok(()),
    }
}

/// `PUT /v1/pipelines/{pipeline_id}/workflow-settings/{workflow}`
pub async fn update_pipeline_workflow_settings(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((pipeline_id, workflow)): Path<(String, String)>,
    Json(req): Json<UpdatePipelineWorkflowSettingsRequest>,
) -> ApiResult<PipelineWorkflowSettings> {
    let project_id = require_pipeline_project_permission(
        &state.db,
        &auth.0.user_id,
        &auth.0.role,
        &auth.0.auth_source,
        &pipeline_id,
        ProjectPermission::ManagePipelines,
    )
    .await?;
    if !is_valid_workflow_name(&workflow) {
        return Err(invalid("workflow is not a valid workflow name"));
    }
    let existing = load_workflow_settings(&state.db, &pipeline_id, &workflow)
        .await
        .map_err(|e| store_error("Failed to load workflow settings", e))?;
    // Changing a gate takes the same role as changing the pipeline's policy.
    if existing.as_ref().and_then(|s| s.approval_required) != req.approval_required {
        require_project(&state, &auth, &project_id, ProjectPermission::ApproveBuild).await?;
    }
    check_identity(
        &state.db,
        &project_id,
        req.android_identity_id.as_deref(),
        SigningIdentityKind::AndroidKeystore,
        "android_identity_id",
    )
    .await?;
    check_identity(
        &state.db,
        &project_id,
        req.ios_identity_id.as_deref(),
        SigningIdentityKind::IosDistribution,
        "ios_identity_id",
    )
    .await?;

    let empty = req.android_identity_id.is_none()
        && req.ios_identity_id.is_none()
        && req.publish_enabled.is_none()
        && req.approval_required.is_none();
    if empty {
        sqlx::query(
            "DELETE FROM pipeline_workflow_settings WHERE pipeline_id = ?1 AND workflow = ?2",
        )
        .bind(&pipeline_id)
        .bind(&workflow)
        .execute(&state.db)
        .await
        .map_err(|e| store_error("Failed to save workflow settings", e))?;
        return Ok(Json(PipelineWorkflowSettings {
            workflow,
            ..PipelineWorkflowSettings::default()
        }));
    }
    sqlx::query(
        "INSERT INTO pipeline_workflow_settings \
         (pipeline_id, workflow, android_identity_id, ios_identity_id, publish_enabled, approval_required, updated_by, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
         ON CONFLICT(pipeline_id, workflow) DO UPDATE SET \
           android_identity_id = excluded.android_identity_id, \
           ios_identity_id = excluded.ios_identity_id, \
           publish_enabled = excluded.publish_enabled, \
           approval_required = excluded.approval_required, \
           updated_by = excluded.updated_by, \
           updated_at = excluded.updated_at",
    )
    .bind(&pipeline_id)
    .bind(&workflow)
    .bind(&req.android_identity_id)
    .bind(&req.ios_identity_id)
    .bind(req.publish_enabled.map(i64::from))
    .bind(req.approval_required.map(i64::from))
    .bind(&auth.0.user_id)
    .bind(now_unix())
    .execute(&state.db)
    .await
    .map_err(|e| store_error("Failed to save workflow settings", e))?;
    let saved = load_workflow_settings(&state.db, &pipeline_id, &workflow)
        .await
        .map_err(|e| store_error("Failed to load workflow settings", e))?
        .expect("just saved");
    Ok(Json(saved))
}

// ── Build binding ───────────────────────────────────────────────

/// What a build of one workflow signs and publishes with, recorded in its
/// snapshot when it is created.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct WorkflowBinding {
    pub android_identity_id: Option<String>,
    pub ios_identity_id: Option<String>,
    pub publish_disabled: bool,
    /// Replaces the pipeline's approval policy.
    pub approval_required: Option<bool>,
}

impl WorkflowBinding {
    pub fn write_to(&self, snapshot: &mut serde_json::Value) {
        let mut identities = serde_json::Map::new();
        if let Some(id) = &self.android_identity_id {
            identities.insert("android".into(), serde_json::json!(id));
        }
        if let Some(id) = &self.ios_identity_id {
            identities.insert("ios".into(), serde_json::json!(id));
        }
        if !identities.is_empty() {
            snapshot["signing_identities"] = serde_json::Value::Object(identities);
        }
        if self.publish_disabled {
            snapshot["publish_disabled"] = serde_json::json!(true);
        }
    }
}

struct NamedIdentity {
    id: String,
    kind: SigningIdentityKind,
    export_method: Option<IosExportMethod>,
}

async fn identity_by(
    pool: &SqlitePool,
    project_id: &str,
    column: &str,
    value: &str,
) -> Result<Option<NamedIdentity>, sqlx::Error> {
    let sql = match column {
        "id" => {
            "SELECT id, kind, summary FROM project_signing_identities WHERE project_id = ?1 AND id = ?2"
        }
        _ => {
            "SELECT id, kind, summary FROM project_signing_identities WHERE project_id = ?1 AND name = ?2"
        }
    };
    Ok(sqlx::query(sql)
        .bind(project_id)
        .bind(value)
        .fetch_optional(pool)
        .await?
        .map(|row| {
            let summary: Summary =
                serde_json::from_str(&row.get::<String, _>("summary")).unwrap_or_default();
            NamedIdentity {
                id: row.get("id"),
                kind: parse_kind(&row.get::<String, _>("kind")),
                export_method: summary.export_method,
            }
        }))
}

/// Resolve the identities a build of `workflow` signs with: the pipeline's
/// workflow settings first, then the names the file gives in `signing`.
/// `Err` is a message for the user: a name the library does not hold, an
/// identity of the wrong kind, or an ad hoc identity for an App Store upload.
pub(crate) async fn resolve_workflow_binding(
    pool: &SqlitePool,
    project_id: &str,
    pipeline_id: &str,
    workflow: Option<&str>,
    file_signing: &SigningSpec,
    needs_app_store_export: bool,
) -> Result<WorkflowBinding, String> {
    let settings = match workflow {
        Some(workflow) => load_workflow_settings(pool, pipeline_id, workflow)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to load workflow settings");
                "Failed to load the workflow settings".to_string()
            })?,
        None => None,
    }
    .unwrap_or_default();
    let scope = workflow
        .map(|name| format!("workflow {name}"))
        .unwrap_or_else(|| "this build".to_string());

    let mut binding = WorkflowBinding {
        publish_disabled: settings.publish_enabled == Some(false),
        approval_required: settings.approval_required,
        ..WorkflowBinding::default()
    };
    for (platform, kind, picked, named) in [
        (
            "android",
            SigningIdentityKind::AndroidKeystore,
            settings.android_identity_id.as_deref(),
            file_signing.android.as_deref(),
        ),
        (
            "ios",
            SigningIdentityKind::IosDistribution,
            settings.ios_identity_id.as_deref(),
            file_signing.ios.as_deref(),
        ),
    ] {
        let lookup = match (picked, named) {
            (Some(id), _) => identity_by(pool, project_id, "id", id).await,
            (None, Some(name)) => identity_by(pool, project_id, "name", name.trim()).await,
            (None, None) => continue,
        }
        .map_err(|e| {
            error!(error = %e, "failed to load a signing identity");
            "Failed to load the signing identities".to_string()
        })?;
        let Some(identity) = lookup else {
            return Err(match named {
                Some(name) if picked.is_none() => format!(
                    "{scope} signs {platform} with '{name}', which is not in the project's signing identities; add it under Project settings > Signing, or fix signing.{platform}"
                ),
                _ => format!(
                    "{scope}'s {platform} signing identity was deleted; pick another in the pipeline's workflow settings"
                ),
            });
        };
        if identity.kind != kind {
            return Err(format!(
                "{scope} signs {platform} with '{}', which is an {} identity",
                named.unwrap_or("the picked identity"),
                kind_str(identity.kind)
            ));
        }
        if kind == SigningIdentityKind::IosDistribution
            && needs_app_store_export
            && !binding.publish_disabled
            && identity.export_method == Some(IosExportMethod::AdHoc)
        {
            return Err(format!(
                "{scope} uploads to App Store Connect, but its iOS signing identity holds ad hoc profiles; use an identity with App Store profiles"
            ));
        }
        match platform {
            "android" => binding.android_identity_id = Some(identity.id),
            _ => binding.ios_identity_id = Some(identity.id),
        }
    }
    Ok(binding)
}

// ── Runner side ─────────────────────────────────────────────────

async fn snapshot_identity(
    pool: &SqlitePool,
    encryption_key: &[u8],
    job_id: &str,
    platform: &str,
) -> Result<Option<Material>, ApiFailure> {
    let snapshot: Option<String> =
        sqlx::query_scalar("SELECT config_snapshot FROM builds WHERE id = ?1")
            .bind(job_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| store_error("Failed to load build signing settings", e))?;
    let snapshot = snapshot.and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok());
    if let Some(message) = snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.get("signing_error"))
        .and_then(serde_json::Value::as_str)
    {
        return Err(api_err(StatusCode::CONFLICT, "invalid_signing", message));
    }
    let Some(identity_id) = snapshot.and_then(|snapshot| {
        snapshot
            .get("signing_identities")
            .and_then(|ids| ids.get(platform))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    }) else {
        return Ok(None);
    };
    let encrypted: Option<String> = sqlx::query_scalar(
        "SELECT i.material_encrypted FROM project_signing_identities i \
         JOIN builds b ON b.project_id = i.project_id \
         WHERE i.id = ?1 AND b.id = ?2",
    )
    .bind(&identity_id)
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| store_error("Failed to load build signing settings", e))?;
    let Some(encrypted) = encrypted else {
        return Err(api_err(
            StatusCode::CONFLICT,
            "signing_identity_missing",
            format!("The {platform} signing identity this build was created with was deleted"),
        ));
    };
    let mut plain = crypto::decrypt(&encrypted, encryption_key).map_err(|e| {
        error!(error = %e, "failed to decrypt a signing identity");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "decryption_error",
            "Failed to load build signing settings",
        )
    })?;
    let material = serde_json::from_str::<Material>(&plain)
        .map_err(|e| store_error("Failed to load build signing settings", e));
    plain.zeroize();
    material.map(Some)
}

/// The release profile a build's Android signing identity holds, when its
/// workflow names one.
pub(crate) async fn job_android_release_profile(
    pool: &SqlitePool,
    encryption_key: &[u8],
    job_id: &str,
) -> Result<Option<RunnerAndroidSigningProfile>, ApiFailure> {
    Ok(
        match snapshot_identity(pool, encryption_key, job_id, "android").await? {
            Some(Material::AndroidKeystore {
                ref keystore_filename,
                ref keystore_base64,
                ref store_password,
                ref key_alias,
                ref key_password,
            }) => Some(RunnerAndroidSigningProfile {
                build_type: AndroidSigningBuildType::Release,
                enabled: true,
                keystore_filename: keystore_filename.clone(),
                keystore_base64: keystore_base64.clone(),
                store_password: store_password.clone(),
                key_alias: key_alias.clone(),
                key_password: key_password.clone(),
            }),
            _ => None,
        },
    )
}

/// The signing bundle a build's iOS signing identity holds, when its
/// workflow names one.
pub(crate) async fn job_ios_bundle(
    pool: &SqlitePool,
    encryption_key: &[u8],
    job_id: &str,
) -> Result<Option<RunnerIosSigningBundle>, ApiFailure> {
    Ok(
        match snapshot_identity(pool, encryption_key, job_id, "ios").await? {
            Some(Material::IosDistribution {
                ref team_id,
                export_method,
                ref p12_filename,
                ref p12_base64,
                ref p12_password,
                ref profiles,
            }) => Some(RunnerIosSigningBundle {
                enabled: true,
                mode: IosSigningMode::Manual,
                team_id: team_id.clone(),
                export_method,
                p12_filename: p12_filename.clone(),
                p12_base64: p12_base64.clone(),
                p12_password: p12_password.clone(),
                provisioning_profiles: profiles.clone(),
            }),
            _ => None,
        },
    )
}

/// Whether a build's workflow settings turn its publishing off.
pub(crate) async fn job_publish_disabled(
    pool: &SqlitePool,
    job_id: &str,
) -> Result<bool, ApiFailure> {
    let disabled: Option<i64> = sqlx::query_scalar(
        "SELECT COALESCE(json_extract(config_snapshot, '$.publish_disabled'), 0) FROM builds WHERE id = ?1",
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| store_error("Failed to load publish settings", e))?;
    Ok(disabled.unwrap_or(0) != 0)
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
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        for statement in [
            "INSERT INTO users (id, email, oidc_subject, role, created_at, updated_at) \
             VALUES ('u1', 'lead@example.com', 'sub', 'owner', 0, 0)",
            "INSERT INTO projects (id, name, created_by, created_at, updated_at) \
             VALUES ('p1', 'App', 'u1', 0, 0)",
            "INSERT INTO projects (id, name, created_by, created_at, updated_at) \
             VALUES ('p2', 'Other', 'u1', 0, 0)",
            "INSERT INTO pipelines (id, project_id, name, trigger_config, created_at, updated_at) \
             VALUES ('pl1', 'p1', 'Coin', '{}', 0, 0)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        pool
    }

    #[allow(clippy::too_many_arguments)]
    async fn add_identity(
        pool: &SqlitePool,
        id: &str,
        project: &str,
        name: &str,
        kind: &str,
        summary: &str,
        key: &[u8],
        material: &Material,
    ) {
        sqlx::query(
            "INSERT INTO project_signing_identities (id, project_id, name, kind, summary, material_encrypted, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 0)",
        )
        .bind(id)
        .bind(project)
        .bind(name)
        .bind(kind)
        .bind(summary)
        .bind(crypto::encrypt(&serde_json::to_string(material).unwrap(), key).unwrap())
        .execute(pool)
        .await
        .unwrap();
    }

    fn keystore() -> Material {
        Material::AndroidKeystore {
            keystore_filename: "upload.jks".into(),
            keystore_base64: "a2V5".into(),
            store_password: "store".into(),
            key_alias: "upload".into(),
            key_password: "key".into(),
        }
    }

    fn ios(export_method: IosExportMethod) -> Material {
        Material::IosDistribution {
            team_id: "TEAM123456".into(),
            export_method,
            p12_filename: "dist.p12".into(),
            p12_base64: "cDEy".into(),
            p12_password: "pw".into(),
            profiles: Vec::new(),
        }
    }

    #[tokio::test]
    async fn workflows_resolve_identities_by_name_or_settings() {
        let pool = pool().await;
        let key = [3u8; 32];
        add_identity(
            &pool,
            "a1",
            "p1",
            "coin-upload",
            "android_keystore",
            "{}",
            &key,
            &keystore(),
        )
        .await;
        add_identity(
            &pool,
            "i-adhoc",
            "p1",
            "coin-adhoc",
            "ios_distribution",
            r#"{"export_method":"ad_hoc"}"#,
            &key,
            &ios(IosExportMethod::AdHoc),
        )
        .await;
        add_identity(
            &pool,
            "i-store",
            "p1",
            "coin-store",
            "ios_distribution",
            r#"{"export_method":"app_store"}"#,
            &key,
            &ios(IosExportMethod::AppStore),
        )
        .await;
        add_identity(
            &pool,
            "x1",
            "p2",
            "elsewhere",
            "android_keystore",
            "{}",
            &key,
            &keystore(),
        )
        .await;

        let named = SigningSpec {
            android: Some("coin-upload".into()),
            ios: Some("coin-store".into()),
        };
        let binding = resolve_workflow_binding(&pool, "p1", "pl1", Some("release"), &named, true)
            .await
            .unwrap();
        assert_eq!(binding.android_identity_id.as_deref(), Some("a1"));
        assert_eq!(binding.ios_identity_id.as_deref(), Some("i-store"));
        assert!(!binding.publish_disabled);

        // Another project's identity is not found by name.
        let foreign = SigningSpec {
            android: Some("elsewhere".into()),
            ios: None,
        };
        let error = resolve_workflow_binding(&pool, "p1", "pl1", Some("release"), &foreign, false)
            .await
            .unwrap_err();
        assert!(
            error.contains("'elsewhere', which is not in the project's signing identities"),
            "{error}"
        );

        // Ad hoc profiles cannot upload to App Store Connect.
        let adhoc = SigningSpec {
            android: None,
            ios: Some("coin-adhoc".into()),
        };
        let error = resolve_workflow_binding(&pool, "p1", "pl1", Some("release"), &adhoc, true)
            .await
            .unwrap_err();
        assert!(error.contains("holds ad hoc profiles"), "{error}");
        // A kind mismatch is named.
        let wrong = SigningSpec {
            android: Some("coin-store".into()),
            ios: None,
        };
        assert!(
            resolve_workflow_binding(&pool, "p1", "pl1", None, &wrong, false)
                .await
                .unwrap_err()
                .contains("ios_distribution identity")
        );

        // Workflow settings replace the file's names and can stop publishing.
        sqlx::query(
            "INSERT INTO pipeline_workflow_settings (pipeline_id, workflow, ios_identity_id, publish_enabled, updated_at) \
             VALUES ('pl1', 'adhoc', 'i-adhoc', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let binding = resolve_workflow_binding(&pool, "p1", "pl1", Some("adhoc"), &named, true)
            .await
            .unwrap();
        assert_eq!(binding.ios_identity_id.as_deref(), Some("i-adhoc"));
        assert!(binding.publish_disabled);

        // The runner gets the material of the identity in the snapshot.
        let mut snapshot = serde_json::json!({});
        WorkflowBinding {
            android_identity_id: Some("a1".into()),
            ios_identity_id: Some("i-store".into()),
            publish_disabled: true,
            approval_required: None,
        }
        .write_to(&mut snapshot);
        sqlx::query(
            "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, config_snapshot, queued_at, created_at, updated_at) \
             VALUES ('b1', 'p1', 'pl1', 1, 'running', 'manual', ?1, 0, 0, 0)",
        )
        .bind(snapshot.to_string())
        .execute(&pool)
        .await
        .unwrap();
        let android = job_android_release_profile(&pool, &key, "b1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(android.key_alias, "upload");
        let bundle = job_ios_bundle(&pool, &key, "b1").await.unwrap().unwrap();
        assert_eq!(bundle.export_method, IosExportMethod::AppStore);
        assert_eq!(bundle.mode, IosSigningMode::Manual);
        assert!(job_publish_disabled(&pool, "b1").await.unwrap());
    }
}
