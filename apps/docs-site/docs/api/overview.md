# API Reference

The oore.build daemon (`oored`) exposes a RESTful JSON API on `http://127.0.0.1:8787` by default.

## Base URL

```
http://127.0.0.1:8787
```

All API endpoints are versioned under `/v1`.

## Authentication

The API uses two types of authentication depending on the context:

| Context | Auth Type | Header |
|---|---|---|
| Setup endpoints | Setup session token | `Authorization: Bearer <setup_session_token>` |
| Auth endpoints | User session token | `Authorization: Bearer <user_session_token>` |
| Public endpoints | None | -- |

Setup session tokens are obtained by [verifying a bootstrap token](/api/setup#verify-bootstrap-token) and have a 30-minute sliding-window TTL.

User session tokens are obtained through the [OIDC callback](/api/auth#oidc-callback) and have a 24-hour TTL.

## Error Format

All error responses use a consistent JSON structure:

```json
{
  "error": "Human-readable error message",
  "code": "machine_readable_error_code",
  "details": "Optional additional context"
}
```

The `details` field is omitted when not applicable.

### Common Error Codes

| HTTP Status | Code | Description |
|---|---|---|
| 401 | `missing_auth` | Authorization header not provided |
| 401 | `invalid_session` | Session token is invalid |
| 401 | `session_expired` | Session token has expired |
| 401 | `no_session` | No active setup session exists |
| 409 | `already_configured` | Setup is already complete (state is `ready`) |
| 409 | `invalid_state` | Operation not valid in the current setup state |
| 409 | `setup_incomplete` | Auth endpoints require setup to be complete |
| 500 | `store_error` | Database or storage error |

## CORS

The API is configured with the following CORS policy:

- **Allowed origins**: `http://localhost:3000`
- **Allowed methods**: `GET`, `POST`, `OPTIONS`
- **Allowed headers**: `Content-Type`, `Authorization`

## Endpoint Groups

### [Setup API](/api/setup)

Endpoints for first-run instance configuration. All mutating setup endpoints require a valid setup session token and are permanently disabled after the instance reaches `ready` state.

| Method | Path | Auth |
|---|---|---|
| `GET` | [`/v1/public/setup-status`](/api/setup#setup-status) | Public |
| `POST` | [`/v1/setup/bootstrap-token/verify`](/api/setup#verify-bootstrap-token) | Public |
| `POST` | [`/v1/setup/oidc/configure`](/api/setup#configure-oidc) | Setup session |
| `POST` | [`/v1/setup/owner/start-oidc`](/api/setup#start-owner-oidc) | Setup session |
| `POST` | [`/v1/setup/owner/verify-oidc`](/api/setup#verify-owner-oidc) | Setup session |
| `POST` | [`/v1/setup/complete`](/api/setup#complete-setup) | Setup session |

### [Auth API](/api/auth)

Endpoints for OIDC authentication and session management. Only available when setup is complete (`setup_state == Ready`).

| Method | Path | Auth |
|---|---|---|
| `GET` | [`/v1/auth/oidc/start`](/api/auth#oidc-start) | Public |
| `GET` | [`/v1/auth/oidc/callback`](/api/auth#oidc-callback) | Public |
| `POST` | [`/v1/auth/logout`](/api/auth#logout) | User session |

### Health Check

| Method | Path | Auth |
|---|---|---|
| `GET` | `/healthz` | Public |

Returns `{"ok": true}`.
