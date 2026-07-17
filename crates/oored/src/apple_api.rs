use axum::Json;
use axum::http::StatusCode;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use oore_contract::ApiError;
use serde::{Deserialize, Serialize};
use tracing::{error, warn};
use uuid::Uuid;

use crate::util::{api_err, now_unix};

const DEFAULT_APP_STORE_CONNECT_API_BASE: &str = "https://api.appstoreconnect.apple.com";

#[derive(Debug, Clone)]
pub struct AppleApiCredentials {
    pub key_id: String,
    pub issuer_id: String,
    pub private_key_pem: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppleDeviceRecord {
    pub device_id: String,
    pub udid: String,
    pub name: String,
    pub platform: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct AppleBundleIdRecord {
    pub bundle_id_id: String,
    pub identifier: String,
    pub platform: String,
}

#[derive(Debug, Clone)]
pub struct AppleCertificateRecord {
    pub certificate_id: String,
    pub certificate_content: Option<String>,
    pub certificate_type: Option<String>,
    pub serial_number: Option<String>,
    pub expiration_date: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AppleProfileRecord {
    pub profile_id: String,
    pub name: String,
    pub uuid: Option<String>,
    pub expiration_date: Option<String>,
    pub profile_content: Option<String>,
}

#[derive(Serialize)]
struct AppleJwtClaims {
    iss: String,
    iat: i64,
    exp: i64,
    aud: &'static str,
}

#[derive(Debug, Deserialize)]
struct AppleApiListResponse<T> {
    data: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct AppleApiDataResponse<T> {
    data: T,
}

#[derive(Debug, Deserialize)]
struct AppleDeviceData {
    id: String,
    attributes: AppleDeviceAttributes,
}

#[derive(Debug, Deserialize)]
struct AppleDeviceAttributes {
    name: String,
    udid: String,
    platform: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct AppleBundleIdData {
    id: String,
    attributes: AppleBundleIdAttributes,
}

#[derive(Debug, Deserialize)]
struct AppleBundleIdAttributes {
    identifier: String,
    platform: String,
}

#[derive(Debug, Deserialize)]
struct AppleCertificateData {
    id: String,
    attributes: AppleCertificateAttributes,
}

#[derive(Debug, Deserialize)]
struct AppleCertificateAttributes {
    #[serde(rename = "certificateContent")]
    certificate_content: Option<String>,
    #[serde(rename = "certificateType")]
    certificate_type: Option<String>,
    #[serde(rename = "serialNumber")]
    serial_number: Option<String>,
    #[serde(rename = "expirationDate")]
    expiration_date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AppleProfileData {
    id: String,
    attributes: AppleProfileAttributes,
}

#[derive(Debug, Deserialize)]
struct AppleProfileAttributes {
    name: String,
    uuid: Option<String>,
    #[serde(rename = "expirationDate")]
    expiration_date: Option<String>,
    #[serde(rename = "profileContent")]
    profile_content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AppleApiErrorEnvelope {
    errors: Vec<AppleApiError>,
}

#[derive(Debug, Deserialize)]
struct AppleApiError {
    code: Option<String>,
    detail: Option<String>,
    title: Option<String>,
}

fn app_store_connect_api_base() -> String {
    std::env::var("OORE_APP_STORE_CONNECT_API_BASE_URL")
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_APP_STORE_CONNECT_API_BASE.to_string())
}

fn endpoint(path: &str) -> String {
    format!("{}{}", app_store_connect_api_base(), path)
}

fn generate_app_store_connect_jwt(
    creds: &AppleApiCredentials,
) -> Result<String, (StatusCode, Json<ApiError>)> {
    let now = now_unix();
    let claims = AppleJwtClaims {
        iss: creds.issuer_id.clone(),
        iat: now - 60,
        exp: now + (15 * 60),
        aud: "appstoreconnect-v1",
    };

    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(creds.key_id.clone());

    let key = EncodingKey::from_ec_pem(creds.private_key_pem.as_bytes()).map_err(|e| {
        error!(error = %e, "failed to parse App Store Connect private key");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "key_error",
            "Invalid App Store Connect API private key",
        )
    })?;

    jsonwebtoken::encode(&header, &claims, &key).map_err(|e| {
        error!(error = %e, "failed to generate App Store Connect JWT");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "jwt_error",
            "Failed to generate App Store Connect token",
        )
    })
}

fn parse_apple_error(body: &str) -> String {
    let parsed: Result<AppleApiErrorEnvelope, _> = serde_json::from_str(body);
    match parsed {
        Ok(payload) => {
            if let Some(first) = payload.errors.first() {
                let code = first.code.as_deref().unwrap_or("unknown");
                let detail = first
                    .detail
                    .as_deref()
                    .or(first.title.as_deref())
                    .unwrap_or("Apple API request failed");
                format!("{code}: {detail}")
            } else {
                "Apple API request failed".to_string()
            }
        }
        Err(_) => "Apple API request failed".to_string(),
    }
}

fn should_retry_profile_create_after_conflict(body: &str) -> bool {
    let parsed: Result<AppleApiErrorEnvelope, _> = serde_json::from_str(body);
    match parsed {
        Ok(payload) => payload.errors.iter().any(|entry| {
            let combined = format!(
                "{} {}",
                entry.title.as_deref().unwrap_or_default(),
                entry.detail.as_deref().unwrap_or_default()
            )
            .to_ascii_lowercase();
            combined.contains("profile")
                && combined.contains("name")
                && (combined.contains("already exists")
                    || combined.contains("duplicate")
                    || combined.contains("already in use"))
        }),
        Err(_) => false,
    }
}

fn map_upstream_error(
    scope: &str,
    status: reqwest::StatusCode,
    body: &str,
) -> (StatusCode, Json<ApiError>) {
    api_err(
        StatusCode::BAD_GATEWAY,
        "apple_api_error",
        format!("{scope} failed ({status}): {}", parse_apple_error(body)),
    )
}

fn to_device_record(data: AppleDeviceData) -> AppleDeviceRecord {
    AppleDeviceRecord {
        device_id: data.id,
        udid: data.attributes.udid,
        name: data.attributes.name,
        platform: data.attributes.platform,
        status: data.attributes.status,
    }
}

fn to_bundle_id_record(data: AppleBundleIdData) -> AppleBundleIdRecord {
    AppleBundleIdRecord {
        bundle_id_id: data.id,
        identifier: data.attributes.identifier,
        platform: data.attributes.platform,
    }
}

fn to_certificate_record(data: AppleCertificateData) -> AppleCertificateRecord {
    AppleCertificateRecord {
        certificate_id: data.id,
        certificate_content: data.attributes.certificate_content,
        certificate_type: data.attributes.certificate_type,
        serial_number: data.attributes.serial_number,
        expiration_date: data.attributes.expiration_date,
    }
}

fn to_profile_record(data: AppleProfileData) -> AppleProfileRecord {
    AppleProfileRecord {
        profile_id: data.id,
        name: data.attributes.name,
        uuid: data.attributes.uuid,
        expiration_date: data.attributes.expiration_date,
        profile_content: data.attributes.profile_content,
    }
}

pub async fn list_devices(
    client: &reqwest::Client,
    creds: &AppleApiCredentials,
) -> Result<Vec<AppleDeviceRecord>, (StatusCode, Json<ApiError>)> {
    let jwt = generate_app_store_connect_jwt(creds)?;
    let resp = client
        .get(endpoint("/v1/devices"))
        .query(&[
            ("limit", "200"),
            ("fields[devices]", "name,platform,udid,status,addedDate"),
        ])
        .bearer_auth(jwt)
        .send()
        .await
        .map_err(|e| {
            error!(error = %e, "failed to call App Store Connect devices API");
            api_err(
                StatusCode::BAD_GATEWAY,
                "apple_api_error",
                "Failed to list devices from Apple",
            )
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(map_upstream_error("Apple device list", status, &body));
    }

    let payload: AppleApiListResponse<AppleDeviceData> = resp.json().await.map_err(|e| {
        error!(error = %e, "failed to parse App Store Connect device list response");
        api_err(
            StatusCode::BAD_GATEWAY,
            "apple_api_error",
            "Invalid response from Apple device list",
        )
    })?;

    Ok(payload.data.into_iter().map(to_device_record).collect())
}

pub async fn register_device(
    client: &reqwest::Client,
    creds: &AppleApiCredentials,
    udid: &str,
    name: &str,
    platform: &str,
) -> Result<AppleDeviceRecord, (StatusCode, Json<ApiError>)> {
    let jwt = generate_app_store_connect_jwt(creds)?;
    let normalized_platform = match platform.to_ascii_uppercase().as_str() {
        "IOS" => "IOS",
        "IPADOS" => "IPADOS",
        "TVOS" => "TVOS",
        "WATCHOS" => "WATCHOS",
        "VISIONOS" => "VISIONOS",
        _ => "IOS",
    };

    let body = serde_json::json!({
        "data": {
            "type": "devices",
            "attributes": {
                "name": name,
                "udid": udid,
                "platform": normalized_platform,
            }
        }
    });

    let resp = client
        .post(endpoint("/v1/devices"))
        .bearer_auth(jwt)
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            error!(error = %e, "failed to call App Store Connect register device API");
            api_err(
                StatusCode::BAD_GATEWAY,
                "apple_api_error",
                "Failed to register device with Apple",
            )
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(map_upstream_error(
            "Apple device registration",
            status,
            &body,
        ));
    }

    let payload: AppleApiDataResponse<AppleDeviceData> = resp.json().await.map_err(|e| {
        error!(error = %e, "failed to parse App Store Connect register device response");
        api_err(
            StatusCode::BAD_GATEWAY,
            "apple_api_error",
            "Invalid response from Apple device registration",
        )
    })?;

    Ok(to_device_record(payload.data))
}

pub async fn list_bundle_ids(
    client: &reqwest::Client,
    creds: &AppleApiCredentials,
    bundle_identifiers: &[String],
) -> Result<Vec<AppleBundleIdRecord>, (StatusCode, Json<ApiError>)> {
    let jwt = generate_app_store_connect_jwt(creds)?;
    let mut req = client
        .get(endpoint("/v1/bundleIds"))
        .query(&[
            ("limit", "200"),
            ("fields[bundleIds]", "identifier,name,platform"),
            ("filter[platform]", "IOS"),
        ])
        .bearer_auth(jwt);

    if !bundle_identifiers.is_empty() {
        req = req.query(&[("filter[identifier]", bundle_identifiers.join(","))]);
    }

    let resp = req.send().await.map_err(|e| {
        error!(error = %e, "failed to call App Store Connect bundleIds API");
        api_err(
            StatusCode::BAD_GATEWAY,
            "apple_api_error",
            "Failed to list bundle identifiers from Apple",
        )
    })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(map_upstream_error(
            "Apple bundle identifier lookup",
            status,
            &body,
        ));
    }

    let payload: AppleApiListResponse<AppleBundleIdData> = resp.json().await.map_err(|e| {
        error!(error = %e, "failed to parse App Store Connect bundleIds response");
        api_err(
            StatusCode::BAD_GATEWAY,
            "apple_api_error",
            "Invalid response from Apple bundle identifier lookup",
        )
    })?;

    Ok(payload.data.into_iter().map(to_bundle_id_record).collect())
}

pub async fn list_distribution_certificates(
    client: &reqwest::Client,
    creds: &AppleApiCredentials,
) -> Result<Vec<AppleCertificateRecord>, (StatusCode, Json<ApiError>)> {
    let jwt = generate_app_store_connect_jwt(creds)?;
    let resp = client
        .get(endpoint("/v1/certificates"))
        .query(&[
            ("limit", "200"),
            // Apple may return modern "DISTRIBUTION" and legacy "IOS_DISTRIBUTION".
            ("filter[certificateType]", "DISTRIBUTION,IOS_DISTRIBUTION"),
            (
                "fields[certificates]",
                "certificateType,serialNumber,expirationDate,certificateContent,activated",
            ),
        ])
        .bearer_auth(jwt)
        .send()
        .await
        .map_err(|e| {
            error!(error = %e, "failed to call App Store Connect certificates API");
            api_err(
                StatusCode::BAD_GATEWAY,
                "apple_api_error",
                "Failed to list certificates from Apple",
            )
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(map_upstream_error("Apple certificate list", status, &body));
    }

    let payload: AppleApiListResponse<AppleCertificateData> = resp.json().await.map_err(|e| {
        error!(error = %e, "failed to parse App Store Connect certificates response");
        api_err(
            StatusCode::BAD_GATEWAY,
            "apple_api_error",
            "Invalid response from Apple certificate list",
        )
    })?;

    Ok(payload
        .data
        .into_iter()
        .map(to_certificate_record)
        .collect())
}

pub async fn create_distribution_certificate(
    client: &reqwest::Client,
    creds: &AppleApiCredentials,
    csr_content: &str,
) -> Result<AppleCertificateRecord, (StatusCode, Json<ApiError>)> {
    async fn create_certificate_with_type(
        client: &reqwest::Client,
        jwt: &str,
        csr_content: &str,
        certificate_type: &str,
    ) -> Result<AppleCertificateRecord, (StatusCode, Json<ApiError>)> {
        let body = serde_json::json!({
            "data": {
                "type": "certificates",
                "attributes": {
                    "certificateType": certificate_type,
                    "csrContent": csr_content,
                }
            }
        });

        let resp = client
            .post(endpoint("/v1/certificates"))
            .bearer_auth(jwt)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                error!(
                    error = %e,
                    certificate_type = %certificate_type,
                    "failed to call App Store Connect create certificate API"
                );
                api_err(
                    StatusCode::BAD_GATEWAY,
                    "apple_api_error",
                    "Failed to create iOS distribution certificate",
                )
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(map_upstream_error(
                "Apple certificate creation",
                status,
                &body,
            ));
        }

        let payload: AppleApiDataResponse<AppleCertificateData> =
            resp.json().await.map_err(|e| {
                error!(
                    error = %e,
                    certificate_type = %certificate_type,
                    "failed to parse App Store Connect certificate creation response"
                );
                api_err(
                    StatusCode::BAD_GATEWAY,
                    "apple_api_error",
                    "Invalid response from Apple certificate creation",
                )
            })?;

        Ok(to_certificate_record(payload.data))
    }

    let jwt = generate_app_store_connect_jwt(creds)?;
    let mut last_err: Option<(StatusCode, Json<ApiError>)> = None;

    for certificate_type in ["DISTRIBUTION", "IOS_DISTRIBUTION"] {
        match create_certificate_with_type(client, &jwt, csr_content, certificate_type).await {
            Ok(record) => return Ok(record),
            Err(err) => last_err = Some(err),
        }
    }

    Err(last_err.unwrap_or_else(|| {
        api_err(
            StatusCode::BAD_GATEWAY,
            "apple_api_error",
            "Apple certificate creation failed",
        )
    }))
}

pub async fn create_ad_hoc_profile(
    client: &reqwest::Client,
    creds: &AppleApiCredentials,
    profile_name: &str,
    bundle_id_id: &str,
    certificate_ids: &[String],
    device_ids: &[String],
) -> Result<AppleProfileRecord, (StatusCode, Json<ApiError>)> {
    let jwt = generate_app_store_connect_jwt(creds)?;

    let device_data: Vec<serde_json::Value> = device_ids
        .iter()
        .map(|id| serde_json::json!({ "type": "devices", "id": id }))
        .collect();
    let certificate_data: Vec<serde_json::Value> = certificate_ids
        .iter()
        .map(|id| serde_json::json!({ "type": "certificates", "id": id }))
        .collect();

    let mut relationships = serde_json::Map::new();
    relationships.insert(
        "bundleId".to_string(),
        serde_json::json!({ "data": { "type": "bundleIds", "id": bundle_id_id } }),
    );
    relationships.insert(
        "certificates".to_string(),
        serde_json::json!({ "data": certificate_data }),
    );
    if !device_data.is_empty() {
        relationships.insert(
            "devices".to_string(),
            serde_json::json!({ "data": device_data }),
        );
    }

    let mut body = serde_json::json!({
        "data": {
            "type": "profiles",
            "attributes": {
                "name": profile_name,
                "profileType": "IOS_APP_ADHOC"
            },
            "relationships": relationships
        }
    });

    let resp = client
        .post(endpoint("/v1/profiles"))
        .bearer_auth(jwt)
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            error!(error = %e, "failed to call App Store Connect create profile API");
            api_err(
                StatusCode::BAD_GATEWAY,
                "apple_api_error",
                "Failed to create iOS ad hoc provisioning profile",
            )
        })?;

    let resp_status = resp.status();
    if resp_status == reqwest::StatusCode::CONFLICT {
        let body_text = resp.text().await.unwrap_or_default();
        if !should_retry_profile_create_after_conflict(&body_text) {
            return Err(map_upstream_error(
                "Apple profile creation",
                resp_status,
                &body_text,
            ));
        }
        warn!(
            profile_name = %profile_name,
            response = %body_text,
            "Apple profile creation returned 409 Conflict; retrying with a unique name"
        );
        body["data"]["attributes"]["name"] =
            serde_json::Value::String(format!("oore-adhoc-{}", Uuid::new_v4()));
        let retry_jwt = generate_app_store_connect_jwt(creds)?;
        let retry_resp = client
            .post(endpoint("/v1/profiles"))
            .bearer_auth(retry_jwt)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                error!(error = %e, "failed to retry App Store Connect create profile API");
                api_err(
                    StatusCode::BAD_GATEWAY,
                    "apple_api_error",
                    "Failed to create iOS ad hoc provisioning profile on retry",
                )
            })?;
        if !retry_resp.status().is_success() {
            let retry_status = retry_resp.status();
            let retry_body = retry_resp.text().await.unwrap_or_default();
            return Err(map_upstream_error(
                "Apple profile creation (retry with unique name)",
                retry_status,
                &retry_body,
            ));
        }
        let payload: AppleApiDataResponse<AppleProfileData> =
            retry_resp.json().await.map_err(|e| {
                error!(error = %e, "failed to parse App Store Connect profile creation retry response");
                api_err(
                    StatusCode::BAD_GATEWAY,
                    "apple_api_error",
                    "Invalid response from Apple profile creation retry",
                )
            })?;
        return Ok(to_profile_record(payload.data));
    }

    if !resp_status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(map_upstream_error(
            "Apple profile creation",
            resp_status,
            &body,
        ));
    }

    let payload: AppleApiDataResponse<AppleProfileData> = resp.json().await.map_err(|e| {
        error!(error = %e, "failed to parse App Store Connect profile creation response");
        api_err(
            StatusCode::BAD_GATEWAY,
            "apple_api_error",
            "Invalid response from Apple profile creation",
        )
    })?;

    Ok(to_profile_record(payload.data))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use axum::Json as AxumJson;
    use axum::Router;
    use axum::http::StatusCode;
    use axum::routing::{delete, post};

    use super::{
        AppleApiCredentials, create_ad_hoc_profile, should_retry_profile_create_after_conflict,
    };

    static APPLE_API_ENV_LOCK: Mutex<()> = Mutex::new(());

    struct AppleApiEnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl AppleApiEnvGuard {
        fn set(value: &str) -> Self {
            let lock = APPLE_API_ENV_LOCK.lock().expect("Apple API env lock");
            unsafe { std::env::set_var("OORE_APP_STORE_CONNECT_API_BASE_URL", value) };
            Self { _lock: lock }
        }
    }

    impl Drop for AppleApiEnvGuard {
        fn drop(&mut self) {
            unsafe { std::env::remove_var("OORE_APP_STORE_CONNECT_API_BASE_URL") };
        }
    }

    #[test]
    fn retries_only_for_duplicate_profile_name_conflicts() {
        let duplicate_name = r#"{
            "errors": [{
                "code": "ENTITY_ERROR.ATTRIBUTE.INVALID",
                "title": "Invalid",
                "detail": "A profile with this name already exists."
            }]
        }"#;
        assert!(should_retry_profile_create_after_conflict(duplicate_name));

        let invalid_certificate = r#"{
            "errors": [{
                "code": "ENTITY_ERROR.ATTRIBUTE.INVALID",
                "title": "Invalid Certificate",
                "detail": "Invalid Certificate"
            }]
        }"#;
        assert!(!should_retry_profile_create_after_conflict(
            invalid_certificate
        ));
    }

    #[tokio::test]
    async fn duplicate_profile_name_retry_never_deletes_existing_profiles() {
        let post_count = Arc::new(AtomicUsize::new(0));
        let delete_count = Arc::new(AtomicUsize::new(0));
        let requested_names = Arc::new(Mutex::new(Vec::new()));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock Apple API");
        let address = listener.local_addr().expect("mock Apple API address");
        let post_count_for_route = Arc::clone(&post_count);
        let delete_count_for_route = Arc::clone(&delete_count);
        let requested_names_for_route = Arc::clone(&requested_names);
        let mock = Router::new().route(
            "/v1/profiles",
            post(move |AxumJson(body): AxumJson<serde_json::Value>| {
                let post_count = Arc::clone(&post_count_for_route);
                let requested_names = Arc::clone(&requested_names_for_route);
                async move {
                    let name = body["data"]["attributes"]["name"]
                        .as_str()
                        .expect("profile name")
                        .to_string();
                    requested_names
                        .lock()
                        .expect("requested profile names")
                        .push(name.clone());
                    if post_count.fetch_add(1, Ordering::SeqCst) == 0 {
                        (
                            StatusCode::CONFLICT,
                            AxumJson(serde_json::json!({
                                "errors": [{"detail": "A profile with this name already exists."}]
                            })),
                        )
                    } else {
                        (
                            StatusCode::CREATED,
                            AxumJson(serde_json::json!({
                                "data": {
                                    "id": "new-profile",
                                    "attributes": {
                                        "name": name,
                                        "uuid": "NEW-PROFILE-UUID",
                                        "expirationDate": "2030-01-01T00:00:00Z",
                                        "profileContent": "cHJvZmlsZQ=="
                                    }
                                }
                            })),
                        )
                    }
                }
            })
            .get(|| async {
                AxumJson(serde_json::json!({
                    "data": [{
                        "id": "foreign-profile",
                        "attributes": {
                            "name": "oore-adhoc-com-example-foo-bar",
                            "uuid": "FOREIGN-PROFILE-UUID"
                        }
                    }]
                }))
            }),
        )
        .route(
            "/v1/profiles/{profile_id}",
            delete(move || {
                let delete_count = Arc::clone(&delete_count_for_route);
                async move {
                    delete_count.fetch_add(1, Ordering::SeqCst);
                    StatusCode::NO_CONTENT
                }
            }),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, mock).await.expect("mock Apple API");
        });
        let _env = AppleApiEnvGuard::set(&format!("http://{address}"));

        let creds = AppleApiCredentials {
            key_id: "KEY123".to_string(),
            issuer_id: "00000000-0000-0000-0000-000000000000".to_string(),
            private_key_pem: "-----BEGIN PRIVATE KEY-----\nMIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgEcE27O5vCf64/k4O\n7U3hR/7SFXFrdGszvS0RZiIoXDGhRANCAAQuK9eOqVNMkR4i2w4LK2BSYQVeTch0\nNwWg0o+NYS5KJTd9VTYzKyF4mDmJfq8Jhml8o4vXuFc2vztW/mm35VIo\n-----END PRIVATE KEY-----\n".to_string(),
        };
        let result = create_ad_hoc_profile(
            &reqwest::Client::new(),
            &creds,
            "oore-adhoc-com-example-foo-bar",
            "bundle-id-2",
            &["certificate-id".to_string()],
            &["device-id".to_string()],
        )
        .await
        .expect("unique-name retry succeeds");

        let names = requested_names.lock().expect("requested profile names");
        assert_eq!(names.len(), 2);
        assert_eq!(names[0], "oore-adhoc-com-example-foo-bar");
        assert_ne!(names[1], names[0]);
        assert!(names[1].starts_with("oore-adhoc-"));
        assert_eq!(result.name, names[1]);
        assert_eq!(delete_count.load(Ordering::SeqCst), 0);

        server.abort();
    }
}
