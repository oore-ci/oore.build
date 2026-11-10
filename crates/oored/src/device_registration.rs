//! An iPhone registering itself for a pipeline's ad hoc builds from the
//! install page. The phone installs a short-lived configuration profile of
//! type "Profile Service"; iOS then posts the device's UDID to Oore, which
//! registers it with Apple and refreshes the pipeline's profiles, so the
//! next ad hoc build installs on it.

use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use oore_contract::{
    ApiError, CreateDeviceRegistrationRequest, DeviceRegistration, DeviceRegistrationState,
};
use sqlx::{Row, SqlitePool};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::personal::html_escape;
use crate::project_rbac::{ProjectPermission, require_pipeline_project_permission};
use crate::store::write_audit_log;
use crate::token::{generate_token, hash_token};
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;
type ApiErr = (StatusCode, Json<ApiError>);

const REGISTRATION_TTL_SECS: i64 = 30 * 60;
const MAX_DEVICE_NAME: usize = 50;
/// Profile service callbacks are small signed plists.
pub const MAX_CALLBACK_BYTES: usize = 64 * 1024;

fn store_error(message: &'static str) -> impl FnOnce(sqlx::Error) -> ApiErr {
    move |e| {
        error!(error = %e, "{message}");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", message)
    }
}

fn parse_state(value: &str) -> DeviceRegistrationState {
    match value {
        "registered" => DeviceRegistrationState::Registered,
        "failed" => DeviceRegistrationState::Failed,
        _ => DeviceRegistrationState::Pending,
    }
}

async fn https_base(pool: &SqlitePool) -> Option<String> {
    let settings = crate::instance_settings::load_effective_external_access_network_settings(pool)
        .await
        .ok()?;
    let base = settings.artifact_delivery_url.or(settings.public_url)?;
    base.starts_with("https://")
        .then(|| base.trim_end_matches('/').to_string())
}

/// `POST /v1/pipelines/{pipeline_id}/device-registrations` — anyone who can
/// install the pipeline's builds can register their own iPhone.
pub async fn create_registration(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
    Json(req): Json<CreateDeviceRegistrationRequest>,
) -> ApiResult<DeviceRegistration> {
    require_pipeline_project_permission(
        &state.db,
        &auth.0.user_id,
        &auth.0.role,
        &auth.0.auth_source,
        &pipeline_id,
        ProjectPermission::ReadArtifacts,
    )
    .await?;
    let device_name = req
        .device_name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty());
    if device_name
        .as_ref()
        .is_some_and(|name| name.chars().count() > MAX_DEVICE_NAME)
    {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "device_name must be at most 50 characters",
        ));
    }
    if !crate::pipeline_ios_signing::can_register_devices(&state, &pipeline_id).await {
        return Err(api_err(
            StatusCode::CONFLICT,
            "device_registration_unavailable",
            "This pipeline's iOS signing has no App Store Connect key, so Oore cannot register devices. Ask a maintainer to add your iPhone.",
        ));
    }
    let Some(base) = https_base(&state.db).await else {
        return Err(api_err(
            StatusCode::PRECONDITION_FAILED,
            "external_access_required",
            "Registering an iPhone needs an HTTPS public URL under Settings > General",
        ));
    };
    let id = Uuid::new_v4().to_string();
    let token = generate_token();
    let now = now_unix();
    let expires_at = now + REGISTRATION_TTL_SECS;
    sqlx::query(
        "INSERT INTO ios_device_registrations \
         (id, pipeline_id, token_hash, user_id, device_name, created_at, expires_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )
    .bind(&id)
    .bind(&pipeline_id)
    .bind(hash_token(&token))
    .bind(&auth.0.user_id)
    .bind(&device_name)
    .bind(now)
    .bind(expires_at)
    .execute(&state.db)
    .await
    .map_err(store_error("Failed to start the device registration"))?;
    Ok(Json(DeviceRegistration {
        id,
        pipeline_id,
        state: DeviceRegistrationState::Pending,
        profile_url: Some(format!(
            "{base}/install/register-device/{token}/profile.mobileconfig"
        )),
        device_name,
        error: None,
        created_at: now,
        expires_at,
    }))
}

/// `GET /v1/device-registrations/{registration_id}` — the caller's own
/// registration, for the install page to follow.
pub async fn get_registration(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(registration_id): Path<String>,
) -> ApiResult<DeviceRegistration> {
    let row = sqlx::query(
        "SELECT id, pipeline_id, state, device_name, error, created_at, expires_at \
         FROM ios_device_registrations WHERE id = ?1 AND user_id = ?2",
    )
    .bind(&registration_id)
    .bind(&auth.0.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(store_error("Failed to load the device registration"))?
    .ok_or_else(|| {
        api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Device registration not found",
        )
    })?;
    let expires_at: i64 = row.get("expires_at");
    let mut registration_state = parse_state(&row.get::<String, _>("state"));
    if registration_state == DeviceRegistrationState::Pending && expires_at <= now_unix() {
        registration_state = DeviceRegistrationState::Expired;
    }
    Ok(Json(DeviceRegistration {
        id: row.get("id"),
        pipeline_id: row.get("pipeline_id"),
        state: registration_state,
        profile_url: None,
        device_name: row.get("device_name"),
        error: row.get("error"),
        created_at: row.get("created_at"),
        expires_at,
    }))
}

fn escape_xml(value: &str) -> String {
    html_escape(value)
}

/// The Profile Service payload: iOS asks to install it, then posts the
/// listed device attributes to `callback_url`.
fn profile_service_plist(registration_id: &str, callback_url: &str, app_name: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>PayloadContent</key>
  <dict>
    <key>URL</key>
    <string>{callback}</string>
    <key>DeviceAttributes</key>
    <array>
      <string>UDID</string>
      <string>PRODUCT</string>
      <string>DEVICE_NAME</string>
    </array>
  </dict>
  <key>PayloadOrganization</key>
  <string>Oore</string>
  <key>PayloadDisplayName</key>
  <string>Register this iPhone for {app}</string>
  <key>PayloadDescription</key>
  <string>Sends this iPhone's identifier to Oore so test builds of {app} install on it. Nothing stays installed.</string>
  <key>PayloadVersion</key>
  <integer>1</integer>
  <key>PayloadUUID</key>
  <string>{uuid}</string>
  <key>PayloadIdentifier</key>
  <string>build.oore.device-registration.{id}</string>
  <key>PayloadType</key>
  <string>Profile Service</string>
</dict>
</plist>
"#,
        callback = escape_xml(callback_url),
        app = escape_xml(app_name),
        uuid = Uuid::new_v4().to_string().to_uppercase(),
        id = escape_xml(registration_id),
    )
}

async fn pending_registration(
    pool: &SqlitePool,
    token: &str,
) -> Result<Option<sqlx::sqlite::SqliteRow>, sqlx::Error> {
    sqlx::query(
        "SELECT r.id, r.pipeline_id, r.user_id, r.device_name, r.state, r.expires_at, \
                p.name AS project_name, u.email \
         FROM ios_device_registrations r JOIN pipelines pl ON pl.id = r.pipeline_id \
         JOIN projects p ON p.id = pl.project_id JOIN users u ON u.id = r.user_id \
         WHERE r.token_hash = ?1",
    )
    .bind(hash_token(token))
    .fetch_optional(pool)
    .await
}

fn page(title: &str, detail: &str) -> Response {
    let body = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <meta name=\"robots\" content=\"noindex\"><title>{title}</title></head>\
         <body style=\"font-family:-apple-system,BlinkMacSystemFont,sans-serif;margin:0;\
         padding:48px 24px;text-align:center;color:#1a1a1a\">\
         <h1 style=\"font-size:22px;margin:0 0 8px\">{title}</h1>\
         <p style=\"margin:0;color:#555\">{detail}</p></body></html>",
        title = html_escape(title),
        detail = html_escape(detail),
    );
    let mut response = Html(body).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response
}

/// `GET /install/register-device/{token}/profile.mobileconfig`
pub async fn profile(State(state): State<Arc<AppState>>, Path(token): Path<String>) -> Response {
    let row = match pending_registration(&state.db, &token).await {
        Ok(Some(row)) => row,
        Ok(None) => return page("Link not found", "Start again from the install page."),
        Err(e) => {
            error!(error = %e, "failed to load a device registration");
            return page("Something went wrong", "Try again in a minute.");
        }
    };
    if row.get::<String, _>("state") != "pending" || row.get::<i64, _>("expires_at") <= now_unix() {
        return page(
            "This link has expired",
            "Start again from the install page.",
        );
    }
    let Some(base) = https_base(&state.db).await else {
        return page(
            "Registration is unavailable",
            "Oore needs an HTTPS public URL.",
        );
    };
    let plist = profile_service_plist(
        &row.get::<String, _>("id"),
        &format!("{base}/install/register-device/{token}/callback"),
        &row.get::<String, _>("project_name"),
    );
    let mut response = (StatusCode::OK, plist).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-apple-aspen-config"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response
}

/// The device attributes in a Profile Service callback. iOS posts a CMS
/// signed message whose content is a plain XML plist, so the plist is read
/// straight out of the bytes.
#[derive(Debug, PartialEq, Eq)]
struct DeviceAttributes {
    udid: String,
    product: Option<String>,
    device_name: Option<String>,
}

fn plist_string(plist: &str, key: &str) -> Option<String> {
    let marker = format!("<key>{key}</key>");
    let after = &plist[plist.find(&marker)? + marker.len()..];
    let start = after.find("<string>")? + "<string>".len();
    let end = after[start..].find("</string>")?;
    // Nothing but whitespace may sit between the key and its value.
    if !after[..start - "<string>".len()].trim().is_empty() {
        return None;
    }
    Some(after[start..start + end].trim().to_string())
}

fn parse_callback(body: &[u8]) -> Option<DeviceAttributes> {
    let text = String::from_utf8_lossy(body);
    let start = text.find("<?xml")?;
    let end = text[start..].find("</plist>")? + start + "</plist>".len();
    let plist = &text[start..end];
    let udid = plist_string(plist, "UDID")?;
    if udid.is_empty()
        || udid.len() > 64
        || !udid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return None;
    }
    Some(DeviceAttributes {
        udid: udid.to_uppercase(),
        product: plist_string(plist, "PRODUCT"),
        device_name: plist_string(plist, "DEVICE_NAME"),
    })
}

async fn finish(pool: &SqlitePool, id: &str, state: &str, udid: Option<&str>, error: Option<&str>) {
    let result = sqlx::query(
        "UPDATE ios_device_registrations SET state = ?1, udid = ?2, error = ?3, completed_at = ?4 \
         WHERE id = ?5",
    )
    .bind(state)
    .bind(udid)
    .bind(error)
    .bind(now_unix())
    .bind(id)
    .execute(pool)
    .await;
    if let Err(e) = result {
        error!(error = %e, "failed to record a device registration result");
    }
}

/// `POST /install/register-device/{token}/callback` — iOS posts the device
/// attributes here. Apple expects a 301 to a page Safari then shows.
pub async fn callback(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
    body: Bytes,
) -> Response {
    let done = format!("/install/register-device/{token}/done");
    let row = match pending_registration(&state.db, &token).await {
        Ok(Some(row)) => row,
        _ => return Redirect::permanent(&done).into_response(),
    };
    let id: String = row.get("id");
    if row.get::<String, _>("state") != "pending" || row.get::<i64, _>("expires_at") <= now_unix() {
        return Redirect::permanent(&done).into_response();
    }
    let Some(device) = parse_callback(&body) else {
        warn!(registration_id = %id, "device registration callback had no UDID");
        finish(
            &state.db,
            &id,
            "failed",
            None,
            Some("The iPhone did not send its identifier."),
        )
        .await;
        return Redirect::permanent(&done).into_response();
    };
    let pipeline_id: String = row.get("pipeline_id");
    let user_id: String = row.get("user_id");
    let email: String = row.get("email");
    let name = row
        .get::<Option<String>, _>("device_name")
        .or(device.device_name.clone())
        .unwrap_or_else(|| format!("{email} iPhone"))
        .chars()
        .take(MAX_DEVICE_NAME)
        .collect::<String>();
    match crate::pipeline_ios_signing::register_device_for_pipeline(
        &state,
        &pipeline_id,
        &device.udid,
        &name,
        "IOS",
        &user_id,
        &email,
    )
    .await
    {
        Ok(_) => {
            info!(registration_id = %id, %pipeline_id, "iPhone registered from the install page");
            finish(&state.db, &id, "registered", Some(&device.udid), None).await;
            let _ = write_audit_log(
                &state.db,
                Some(&user_id),
                "ios_device_self_registered",
                "pipeline",
                Some(&pipeline_id),
                Some(
                    &serde_json::json!({
                        "udid": device.udid,
                        "product": device.product,
                        "device_name": name,
                    })
                    .to_string(),
                ),
            )
            .await;
        }
        Err((_, Json(e))) => {
            warn!(registration_id = %id, error = %e.error, "Apple refused a device registration");
            finish(&state.db, &id, "failed", Some(&device.udid), Some(&e.error)).await;
        }
    }
    Redirect::permanent(&done).into_response()
}

/// `GET /install/register-device/{token}/done` — what Safari shows after
/// the profile step.
pub async fn done(State(state): State<Arc<AppState>>, Path(token): Path<String>) -> Response {
    let row =
        sqlx::query("SELECT state, error FROM ios_device_registrations WHERE token_hash = ?1")
            .bind(hash_token(&token))
            .fetch_optional(&state.db)
            .await;
    match row {
        Ok(Some(row)) => match row.get::<String, _>("state").as_str() {
            "registered" => page(
                "This iPhone is registered",
                "The next test build installs on it. Go back to Oore to follow the build. You can delete the leftover profile in Settings > General > VPN & Device Management.",
            ),
            "failed" => page(
                "Registration failed",
                &row.get::<Option<String>, _>("error")
                    .unwrap_or_else(|| "Apple did not accept the device.".to_string()),
            ),
            _ => page(
                "Almost there",
                "Open Settings, tap Profile Downloaded and install it to finish.",
            ),
        },
        _ => page("Link not found", "Start again from the install page."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_udid_out_of_a_signed_callback() {
        let mut body = vec![0x30, 0x82, 0x01, 0x00];
        body.extend_from_slice(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>PRODUCT</key><string>iPhone15,2</string>
<key>UDID</key>
<string>00008110-001A2B3C4D5E6F70</string>
<key>DEVICE_NAME</key><string>Asha's iPhone</string>
</dict></plist>"#,
        );
        body.extend_from_slice(&[0xa0, 0x82, 0x05]);
        assert_eq!(
            parse_callback(&body),
            Some(DeviceAttributes {
                udid: "00008110-001A2B3C4D5E6F70".to_string(),
                product: Some("iPhone15,2".to_string()),
                device_name: Some("Asha's iPhone".to_string()),
            })
        );
    }

    #[test]
    fn rejects_a_callback_without_a_usable_udid() {
        assert_eq!(parse_callback(b"not a plist"), None);
        assert_eq!(
            parse_callback(
                b"<?xml version=\"1.0\"?><plist><dict><key>UDID</key><string>bad udid!</string></dict></plist>"
            ),
            None
        );
    }

    #[test]
    fn profile_points_at_the_callback() {
        let plist = profile_service_plist("r1", "https://oore.example.com/cb", "Shop & Co");
        assert!(plist.contains("<string>https://oore.example.com/cb</string>"));
        assert!(plist.contains("Shop &amp; Co"));
        assert!(plist.contains("<string>Profile Service</string>"));
    }
}
