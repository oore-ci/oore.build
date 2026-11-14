//! Web push for the tester portal installed on a home screen: testers who
//! turn on notifications hear about each new installable build of their
//! apps, and about builds sent to their tester group.
//!
//! Messages are encrypted per RFC 8291 (`aes128gcm`) and signed with the
//! instance's VAPID key (RFC 8292), which is created on first use.

use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use oore_contract::{
    ApiError, WebPushConfig, WebPushSubscriptionRequest, WebPushUnsubscribeRequest,
};
use ring::rand::SystemRandom;
use ring::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair};
use ring::{aead, agreement, hkdf};
use serde::Deserialize;
use sqlx::{Row, SqlitePool};
use tracing::{debug, error, warn};
use uuid::Uuid;

use crate::AppState;
use crate::crypto;
use crate::extractors::AuthUser;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;
type ApiErr = (StatusCode, Json<ApiError>);

const RECORD_SIZE: u32 = 4096;
const PUSH_TTL_SECS: u32 = 24 * 60 * 60;
const NOTIFIER_INTERVAL: Duration = Duration::from_secs(30);
/// Builds that finished longer ago than this are not announced, so turning
/// the feature on does not replay history.
const ANNOUNCE_WINDOW_SECS: i64 = 60 * 60;
const MAX_ENDPOINT_LEN: usize = 2048;

/// What a notification says and where tapping it goes, as the service
/// worker reads it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PushMessage {
    pub title: String,
    pub body: String,
    /// A path in the web app, such as `/builds/b1`.
    pub url: String,
}

fn store_error(message: &'static str) -> impl FnOnce(sqlx::Error) -> ApiErr {
    move |e| {
        error!(error = %e, "{message}");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", message)
    }
}

// ── Keys ────────────────────────────────────────────────────────

struct VapidKey {
    pair: EcdsaKeyPair,
    public_b64: String,
}

async fn vapid_key(pool: &SqlitePool, encryption_key: &[u8]) -> anyhow::Result<VapidKey> {
    let rng = SystemRandom::new();
    let existing = sqlx::query("SELECT private_key_enc FROM web_push_keys WHERE id = 1")
        .fetch_optional(pool)
        .await?;
    let pkcs8 = match existing {
        Some(row) => {
            let encoded =
                crypto::decrypt(&row.get::<String, _>("private_key_enc"), encryption_key)?;
            URL_SAFE_NO_PAD.decode(encoded)?
        }
        None => {
            let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng)
                .map_err(|_| anyhow::anyhow!("could not generate a VAPID key"))?;
            let pair =
                EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &rng)
                    .map_err(|_| anyhow::anyhow!("generated VAPID key is invalid"))?;
            let public = URL_SAFE_NO_PAD.encode(pair.public_key().as_ref());
            let encrypted =
                crypto::encrypt(&URL_SAFE_NO_PAD.encode(pkcs8.as_ref()), encryption_key)?;
            sqlx::query(
                "INSERT OR IGNORE INTO web_push_keys (id, private_key_enc, public_key, created_at) \
                 VALUES (1, ?1, ?2, ?3)",
            )
            .bind(encrypted)
            .bind(public)
            .bind(now_unix())
            .execute(pool)
            .await?;
            // Another request may have won the insert; read back what is stored.
            return Box::pin(vapid_key(pool, encryption_key)).await;
        }
    };
    let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &pkcs8, &rng)
        .map_err(|_| anyhow::anyhow!("stored VAPID key is invalid"))?;
    let public_b64 = URL_SAFE_NO_PAD.encode(pair.public_key().as_ref());
    Ok(VapidKey { pair, public_b64 })
}

/// `Authorization` header value for a push service at `audience`.
fn vapid_authorization(key: &VapidKey, audience: &str, now: i64) -> anyhow::Result<String> {
    let header = URL_SAFE_NO_PAD.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
    let claims = URL_SAFE_NO_PAD.encode(
        serde_json::json!({
            "aud": audience,
            "exp": now + 12 * 60 * 60,
            "sub": "https://oore.build",
        })
        .to_string(),
    );
    let signing_input = format!("{header}.{claims}");
    let signature = key
        .pair
        .sign(&SystemRandom::new(), signing_input.as_bytes())
        .map_err(|_| anyhow::anyhow!("could not sign the VAPID token"))?;
    Ok(format!(
        "vapid t={signing_input}.{}, k={}",
        URL_SAFE_NO_PAD.encode(signature.as_ref()),
        key.public_b64
    ))
}

// ── Encryption (RFC 8291) ───────────────────────────────────────

struct Len(usize);

impl hkdf::KeyType for Len {
    fn len(&self) -> usize {
        self.0
    }
}

fn hkdf_expand(prk: &hkdf::Prk, info: &[u8], len: usize) -> anyhow::Result<Vec<u8>> {
    let mut out = vec![0u8; len];
    prk.expand(&[info], Len(len))
        .and_then(|okm| okm.fill(&mut out))
        .map_err(|_| anyhow::anyhow!("HKDF expand failed"))?;
    Ok(out)
}

/// Encrypt `plaintext` for a subscription with `ua_public` (its p256dh key)
/// and `auth_secret`, with a given sender key and salt.
fn encrypt_with(
    plaintext: &[u8],
    ua_public: &[u8],
    auth_secret: &[u8],
    as_private: agreement::EphemeralPrivateKey,
    salt: [u8; 16],
) -> anyhow::Result<Vec<u8>> {
    let as_public = as_private
        .compute_public_key()
        .map_err(|_| anyhow::anyhow!("could not derive the sender key"))?;
    let as_public = as_public.as_ref().to_vec();
    let ecdh_secret = agreement::agree_ephemeral(
        as_private,
        &agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, ua_public),
        |secret| secret.to_vec(),
    )
    .map_err(|_| anyhow::anyhow!("the subscription key is not a P-256 point"))?;

    let prk_key = hkdf::Salt::new(hkdf::HKDF_SHA256, auth_secret).extract(&ecdh_secret);
    let mut key_info = b"WebPush: info\0".to_vec();
    key_info.extend_from_slice(ua_public);
    key_info.extend_from_slice(&as_public);
    let ikm = hkdf_expand(&prk_key, &key_info, 32)?;

    let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, &salt).extract(&ikm);
    let cek = hkdf_expand(&prk, b"Content-Encoding: aes128gcm\0", 16)?;
    let nonce = hkdf_expand(&prk, b"Content-Encoding: nonce\0", 12)?;

    let mut record = plaintext.to_vec();
    // One record, so this is the last: delimiter 2, no padding.
    record.push(2);
    let key = aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_128_GCM, &cek)
            .map_err(|_| anyhow::anyhow!("bad content key"))?,
    );
    let nonce =
        aead::Nonce::try_assume_unique_for_key(&nonce).map_err(|_| anyhow::anyhow!("bad nonce"))?;
    key.seal_in_place_append_tag(nonce, aead::Aad::empty(), &mut record)
        .map_err(|_| anyhow::anyhow!("encryption failed"))?;

    let mut body = Vec::with_capacity(16 + 4 + 1 + as_public.len() + record.len());
    body.extend_from_slice(&salt);
    body.extend_from_slice(&RECORD_SIZE.to_be_bytes());
    body.push(as_public.len() as u8);
    body.extend_from_slice(&as_public);
    body.extend_from_slice(&record);
    Ok(body)
}

fn encrypt(plaintext: &[u8], p256dh: &str, auth: &str) -> anyhow::Result<Vec<u8>> {
    let ua_public = URL_SAFE_NO_PAD.decode(p256dh.trim_end_matches('='))?;
    let auth_secret = URL_SAFE_NO_PAD.decode(auth.trim_end_matches('='))?;
    let rng = SystemRandom::new();
    let as_private = agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng)
        .map_err(|_| anyhow::anyhow!("could not generate a sender key"))?;
    let mut salt = [0u8; 16];
    ring::rand::SecureRandom::fill(&rng, &mut salt)
        .map_err(|_| anyhow::anyhow!("could not generate a salt"))?;
    encrypt_with(plaintext, &ua_public, &auth_secret, as_private, salt)
}

// ── Sending ─────────────────────────────────────────────────────

fn audience(endpoint: &str) -> Option<String> {
    let url = url::Url::parse(endpoint).ok()?;
    Some(format!("{}://{}", url.scheme(), url.host_str()?)).map(|origin| match url.port() {
        Some(port) => format!("{origin}:{port}"),
        None => origin,
    })
}

async fn send_to_users(
    pool: &SqlitePool,
    encryption_key: &[u8],
    user_ids: &[String],
    message: &PushMessage,
) -> anyhow::Result<()> {
    if user_ids.is_empty() {
        return Ok(());
    }
    let key = vapid_key(pool, encryption_key).await?;
    let payload = serde_json::to_vec(message)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;
    let now = now_unix();
    for user_id in user_ids {
        let subscriptions = sqlx::query(
            "SELECT id, endpoint, p256dh, auth FROM web_push_subscriptions WHERE user_id = ?1",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await?;
        for subscription in subscriptions {
            let id: String = subscription.get("id");
            let endpoint: String = subscription.get("endpoint");
            let Some(audience) = audience(&endpoint) else {
                continue;
            };
            let body = match encrypt(
                &payload,
                &subscription.get::<String, _>("p256dh"),
                &subscription.get::<String, _>("auth"),
            ) {
                Ok(body) => body,
                Err(error) => {
                    warn!(%error, "dropping a web push subscription with unusable keys");
                    let _ = sqlx::query("DELETE FROM web_push_subscriptions WHERE id = ?1")
                        .bind(&id)
                        .execute(pool)
                        .await;
                    continue;
                }
            };
            let response = client
                .post(&endpoint)
                .header("Authorization", vapid_authorization(&key, &audience, now)?)
                .header("Content-Encoding", "aes128gcm")
                .header("Content-Type", "application/octet-stream")
                .header("TTL", PUSH_TTL_SECS.to_string())
                .header("Urgency", "normal")
                .body(body)
                .send()
                .await;
            match response {
                Ok(response) if response.status().is_success() => {
                    let _ = sqlx::query(
                        "UPDATE web_push_subscriptions SET last_success_at = ?1 WHERE id = ?2",
                    )
                    .bind(now)
                    .bind(&id)
                    .execute(pool)
                    .await;
                }
                Ok(response)
                    if matches!(
                        response.status(),
                        reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::GONE
                    ) =>
                {
                    debug!("web push subscription expired; removing it");
                    let _ = sqlx::query("DELETE FROM web_push_subscriptions WHERE id = ?1")
                        .bind(&id)
                        .execute(pool)
                        .await;
                }
                Ok(response) => {
                    warn!(status = %response.status(), "push service refused a notification");
                }
                Err(error) => warn!(%error, "failed to reach a push service"),
            }
        }
    }
    Ok(())
}

/// Push `message` to every browser these people subscribed, in the
/// background.
pub fn push_to_users(state: &AppState, user_ids: &[String], message: &PushMessage) {
    let pool = state.db.clone();
    let key = state.encryption_key.to_vec();
    let user_ids = user_ids.to_vec();
    let message = message.clone();
    tokio::spawn(async move {
        if let Err(error) = send_to_users(&pool, &key, &user_ids, &message).await {
            warn!(%error, "failed to send web push notifications");
        }
    });
}

/// People subscribed to push who can open `project_id`.
async fn subscribers_for_project(
    pool: &SqlitePool,
    project_id: &str,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT DISTINCT s.user_id FROM web_push_subscriptions s JOIN users u ON u.id = s.user_id \
         WHERE u.status = 'active' AND (u.role IN ('owner', 'admin') OR EXISTS ( \
           SELECT 1 FROM project_members pm WHERE pm.user_id = u.id AND pm.project_id = ?1))",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
}

/// Tell subscribed testers about builds that just succeeded with something
/// to install.
async fn announce_new_builds(pool: &SqlitePool, encryption_key: &[u8]) -> anyhow::Result<()> {
    let now = now_unix();
    let rows = sqlx::query(
        "SELECT b.id, b.project_id, b.build_number, b.version_name, b.branch, p.name AS project_name \
         FROM builds b JOIN projects p ON p.id = b.project_id \
         WHERE b.status = 'succeeded' AND b.tester_pushed_at IS NULL \
           AND COALESCE(b.finished_at, b.updated_at) >= ?1 \
           AND EXISTS (SELECT 1 FROM artifacts a WHERE a.build_id = b.id \
             AND a.state = 'available' AND a.artifact_type IN ('apk', 'ipa')) \
         ORDER BY b.finished_at LIMIT 50",
    )
    .bind(now - ANNOUNCE_WINDOW_SECS)
    .fetch_all(pool)
    .await?;
    for row in rows {
        let build_id: String = row.get("id");
        let marked = sqlx::query(
            "UPDATE builds SET tester_pushed_at = ?1 WHERE id = ?2 AND tester_pushed_at IS NULL",
        )
        .bind(now)
        .bind(&build_id)
        .execute(pool)
        .await?
        .rows_affected();
        if marked == 0 {
            continue;
        }
        let users = subscribers_for_project(pool, &row.get::<String, _>("project_id")).await?;
        if users.is_empty() {
            continue;
        }
        let project_name: String = row.get("project_name");
        let version: Option<String> = row.get("version_name");
        let branch: Option<String> = row.get("branch");
        let message = PushMessage {
            title: format!(
                "{project_name} {} is ready to test",
                version.unwrap_or_else(|| format!("#{}", row.get::<i64, _>("build_number")))
            ),
            body: branch
                .map(|branch| format!("Build #{} from {branch}", row.get::<i64, _>("build_number")))
                .unwrap_or_else(|| format!("Build #{}", row.get::<i64, _>("build_number"))),
            url: format!("/builds/{build_id}"),
        };
        send_to_users(pool, encryption_key, &users, &message).await?;
    }
    Ok(())
}

pub fn start_web_push_notifier(pool: SqlitePool, encryption_key: Vec<u8>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(NOTIFIER_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            if let Err(error) = announce_new_builds(&pool, &encryption_key).await {
                warn!(%error, "failed to announce new builds to testers");
            }
        }
    });
}

// ── Handlers ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct WebPushConfigQuery {
    pub endpoint: Option<String>,
}

/// `GET /v1/web-push/config` — the VAPID key, and whether `endpoint` is
/// subscribed for the caller.
pub async fn get_config(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<WebPushConfigQuery>,
) -> ApiResult<WebPushConfig> {
    let key = vapid_key(&state.db, &state.encryption_key)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load the VAPID key");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "web_push_unavailable",
                "Push notifications are not available",
            )
        })?;
    let subscribed = match query.endpoint {
        Some(endpoint) => sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM web_push_subscriptions WHERE endpoint = ?1 AND user_id = ?2)",
        )
        .bind(endpoint)
        .bind(&auth.0.user_id)
        .fetch_one(&state.db)
        .await
        .map_err(store_error("Failed to load push settings"))?,
        None => false,
    };
    Ok(Json(WebPushConfig {
        public_key: key.public_b64,
        subscribed,
    }))
}

fn valid_endpoint(endpoint: &str) -> bool {
    endpoint.len() <= MAX_ENDPOINT_LEN
        && url::Url::parse(endpoint).is_ok_and(|url| url.scheme() == "https")
}

/// `POST /v1/web-push/subscriptions` — subscribe this browser.
pub async fn subscribe(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<WebPushSubscriptionRequest>,
) -> Result<StatusCode, ApiErr> {
    if !valid_endpoint(&req.endpoint)
        || URL_SAFE_NO_PAD
            .decode(req.p256dh.trim_end_matches('='))
            .map_or(true, |key| key.len() != 65)
        || URL_SAFE_NO_PAD
            .decode(req.auth.trim_end_matches('='))
            .map_or(true, |secret| secret.len() != 16)
    {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "The push subscription is not valid",
        ));
    }
    sqlx::query(
        "INSERT INTO web_push_subscriptions (id, user_id, endpoint, p256dh, auth, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT (endpoint) DO UPDATE SET user_id = excluded.user_id, \
           p256dh = excluded.p256dh, auth = excluded.auth",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&auth.0.user_id)
    .bind(&req.endpoint)
    .bind(&req.p256dh)
    .bind(&req.auth)
    .bind(now_unix())
    .execute(&state.db)
    .await
    .map_err(store_error("Failed to save the push subscription"))?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /v1/web-push/unsubscribe` — stop pushing to this browser.
pub async fn unsubscribe(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<WebPushUnsubscribeRequest>,
) -> Result<StatusCode, ApiErr> {
    sqlx::query("DELETE FROM web_push_subscriptions WHERE endpoint = ?1 AND user_id = ?2")
        .bind(&req.endpoint)
        .bind(&auth.0.user_id)
        .execute(&state.db)
        .await
        .map_err(store_error("Failed to remove the push subscription"))?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /v1/web-push/test` — send a test notification to the caller.
pub async fn send_test(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<StatusCode, ApiErr> {
    send_to_users(
        &state.db,
        &state.encryption_key,
        std::slice::from_ref(&auth.0.user_id),
        &PushMessage {
            title: "Notifications are on".to_string(),
            body: "You will hear about new builds here.".to_string(),
            url: "/".to_string(),
        },
    )
    .await
    .map_err(|e| {
        warn!(error = %e, "failed to send a test push");
        api_err(
            StatusCode::BAD_GATEWAY,
            "web_push_failed",
            "The test notification could not be sent",
        )
    })?;
    Ok(StatusCode::ACCEPTED)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A browser's side of RFC 8291: decrypt what `encrypt_with` sent.
    fn decrypt(
        body: &[u8],
        ua_private: agreement::EphemeralPrivateKey,
        ua_public: &[u8],
        auth: &[u8],
    ) -> Vec<u8> {
        let salt = &body[..16];
        let id_len = body[20] as usize;
        let as_public = &body[21..21 + id_len];
        let ecdh_secret = agreement::agree_ephemeral(
            ua_private,
            &agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, as_public),
            |secret| secret.to_vec(),
        )
        .unwrap();
        let prk_key = hkdf::Salt::new(hkdf::HKDF_SHA256, auth).extract(&ecdh_secret);
        let mut key_info = b"WebPush: info\0".to_vec();
        key_info.extend_from_slice(ua_public);
        key_info.extend_from_slice(as_public);
        let ikm = hkdf_expand(&prk_key, &key_info, 32).unwrap();
        let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, salt).extract(&ikm);
        let cek = hkdf_expand(&prk, b"Content-Encoding: aes128gcm\0", 16).unwrap();
        let nonce = hkdf_expand(&prk, b"Content-Encoding: nonce\0", 12).unwrap();
        let key = aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_128_GCM, &cek).unwrap());
        let mut record = body[21 + id_len..].to_vec();
        let plain = key
            .open_in_place(
                aead::Nonce::try_assume_unique_for_key(&nonce).unwrap(),
                aead::Aad::empty(),
                &mut record,
            )
            .unwrap();
        plain.to_vec()
    }

    #[test]
    fn a_subscriber_can_decrypt_the_message() {
        let rng = SystemRandom::new();
        let ua_private =
            agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng).unwrap();
        let ua_public = ua_private.compute_public_key().unwrap().as_ref().to_vec();
        let auth = [7u8; 16];
        let salt = [9u8; 16];
        let as_private =
            agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng).unwrap();
        let plaintext = b"When I grow up, I want to be a watermelon";
        let body = encrypt_with(plaintext, &ua_public, &auth, as_private, salt).unwrap();
        assert_eq!(&body[..16], &salt);
        assert_eq!(u32::from_be_bytes(body[16..20].try_into().unwrap()), 4096);
        assert_eq!(body[20], 65);
        let mut expected = plaintext.to_vec();
        expected.push(2);
        assert_eq!(decrypt(&body, ua_private, &ua_public, &auth), expected);
    }

    #[test]
    fn audience_is_the_push_service_origin() {
        assert_eq!(
            audience("https://fcm.googleapis.com/fcm/send/abc").as_deref(),
            Some("https://fcm.googleapis.com")
        );
        assert!(valid_endpoint("https://web.push.apple.com/abc"));
        assert!(!valid_endpoint("http://example.com/abc"));
    }

    #[test]
    fn vapid_header_names_its_key() {
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap();
        let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &rng)
            .unwrap();
        let public_b64 = URL_SAFE_NO_PAD.encode(pair.public_key().as_ref());
        let key = VapidKey { pair, public_b64 };
        let header = vapid_authorization(&key, "https://push.example", 0).unwrap();
        assert!(header.starts_with("vapid t="));
        assert!(header.ends_with(&format!("k={}", key.public_b64)));
    }
}
