---
status: implemented
description: "API endpoints for OIDC, trusted-proxy, and local session management in Oore CI."
---

# Auth API

Endpoints for OIDC authentication, trusted-proxy authentication, and local session management. These endpoints are only available after setup is complete (instance state is `ready`). Calling them before setup returns `409 Conflict` with code `setup_incomplete`.

## OIDC Start {#oidc-start}

Initiate an OIDC authorization code flow. Returns an authorization URL to redirect the user to.

```
GET /v1/auth/oidc/start
```

**Authentication**: None (public)

### Query parameters

| Parameter | Required | Description |
|---|---|---|
| `redirect_uri` | No | Custom redirect URI (defaults to standard callback) |

### Response `200 OK`

```json
{
  "authorization_url": "https://accounts.google.com/o/oauth2/v2/auth?client_id=...&redirect_uri=...&state=...&nonce=...",
  "state": "abc123-csrf-state-token"
}
```

| Field | Type | Description |
|---|---|---|
| `authorization_url` | `string` | Full authorization URL to redirect the user to |
| `state` | `string` | CSRF state token for validating the callback |

The daemon generates a PKCE challenge (S256), CSRF state token, and nonce internally. Pending auth entries expire after 10 minutes.

### Error responses

| Status | Code | Description |
|---|---|---|
| 409 | `setup_incomplete` | Setup is not yet complete |
| 429 | `too_many_pending` | Too many pending auth requests (limit: 1000) |
| 500 | `oidc_not_configured` | OIDC configuration is missing |
| 502 | `oidc_discovery_error` | Failed to perform OIDC discovery |

---

## OIDC Callback {#oidc-callback}

Complete the OIDC authorization code flow. Exchanges the authorization code for tokens, verifies the ID token, and creates a user session.

```
POST /v1/auth/oidc/callback
```

**Authentication**: None (public)

### Request body

```json
{
  "code": "4/0AX4XfWh...",
  "state": "abc123-csrf-state-token"
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `code` | `string` | Yes | Authorization code from the IdP callback |
| `state` | `string` | Yes | CSRF state token (must match the value from oidc/start) |

### Response `200 OK`

```json
{
  "session_token": "user_session_abc123...",
  "expires_at": 1738886400,
  "user": {
    "email": "dev@example.com",
    "oidc_subject": "110123456789012345678",
    "user_id": "user_def456",
    "role": "developer",
    "avatar_url": "https://lh3.googleusercontent.com/..."
  }
}
```

| Field | Type | Description |
|---|---|---|
| `session_token` | `string` | User session token (24-hour TTL) |
| `expires_at` | `integer` | Session expiry as Unix epoch (seconds) |
| `user` | `object` | Authenticated user details |

### Error responses

| Status | Code | Description |
|---|---|---|
| 400 | `invalid_state` | Unknown or expired OIDC state parameter |
| 400 | `auth_expired` | OIDC authorization request expired (10-minute TTL) |
| 403 | `user_not_found` | No user account for this email/identity |
| 409 | `setup_incomplete` | Setup is not yet complete |
| 500 | `decryption_error` | Failed to decrypt stored OIDC client secret |
| 500 | `session_error` | Failed to create session |
| 500 | `store_error` | Database operation failed |
| 502 | `oidc_discovery_error` | Failed to perform OIDC discovery |
| 502 | `token_exchange_error` | Failed to exchange authorization code |
| 502 | `missing_id_token` | IdP did not return an ID token |
| 502 | `id_token_verification_error` | ID token verification failed |
| 502 | `missing_email` | ID token missing email claim |

::: info
Users must be invited before they can sign in. If a user authenticates successfully with the OIDC provider but has no account in Oore CI, the callback returns `403` with code `user_not_found`.
:::

---

## Local Login {#local-login}

Create a loopback-only local session without OIDC.

```
POST /v1/auth/local/login
```

**Authentication**: None (public)

### Request body

```json
{
  "email": "owner@example.com"
}
```

`email` is optional when exactly one active user exists.

### Response `200 OK`

Returns `LocalLoginResponse`.

### Error responses

| Status | Code | Description |
|---|---|---|
| 400 | `email_required` | Multiple active users exist and email was omitted |
| 403 | `mode_restricted` | Setup is incomplete while runtime mode is `remote` |
| 403 | `local_login_loopback_required` | Local login attempted from non-loopback source |
| 403 | `user_not_found` | No active user matched the provided email |

::: warning
Local login is always loopback-only. Any non-loopback access path must use
External Access (`runtime_mode=remote`) with either OIDC or Trusted Proxy.
:::

---

## Trusted Proxy Login {#trusted-proxy-login}

Create a user session from identity headers asserted by a trusted upstream proxy.

```
POST /v1/auth/trusted-proxy/login
```

**Authentication**: None (public)

### Request body

No request body.

### Required proxy headers

By default, Oore expects the user email in:

```text
X-Oore-User-Email
```

The exact identity header name can be changed in trusted-proxy configuration.
If a shared secret is configured, the proxy must also send:

```text
X-Oore-Trusted-Proxy-Secret
```

### Response `200 OK`

Returns `LocalLoginResponse`.

### Error responses

| Status | Code | Description |
|---|---|---|
| 403 | `mode_restricted` | Instance is not in `runtime_mode=remote` with `remote_auth_mode=trusted_proxy` |
| 403 | `trusted_proxy_peer_not_allowed` | Request did not come from a trusted proxy peer |
| 401 | `trusted_proxy_shared_secret_missing` | Trusted proxy shared secret header is required but missing |
| 401 | `trusted_proxy_shared_secret_invalid` | Trusted proxy shared secret header does not match |
| 401 | `trusted_proxy_identity_missing` | Trusted proxy identity header is missing |
| 401 | `trusted_proxy_identity_invalid` | Trusted proxy identity header is not a valid email |
| 403 | `user_not_found` | No active user matched the forwarded email |

::: info
Trusted Proxy mode keeps authentication at the upstream proxy boundary while preserving normal Oore sessions, RBAC, and audit attribution.
:::

---

## Logout {#logout}

Invalidate the current user session.

```
POST /v1/auth/logout
```

**Authentication**: User session (Bearer)

### Response `200 OK`

```json
{
  "ok": true
}
```

### Error responses

| Status | Code | Description |
|---|---|---|
| 401 | `missing_auth` | Authorization header not provided |
| 401 | `invalid_session` | Session token is invalid or already expired |
