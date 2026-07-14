---
status: implemented
description: 'Complete REST API reference for the Oore CI daemon including endpoints, auth, and error codes.'
---

# API Reference

The Oore CI daemon (`oored`) exposes a RESTful JSON API.

## Base URL

```
http://127.0.0.1:8787
```

All endpoints are versioned under `/v1`. Override the listen address with `--listen` or `OORED_LISTEN_ADDR`.

## Authentication

The API uses different authentication depending on the endpoint group:

| Context          | Auth type           | Header                                        |
| ---------------- | ------------------- | --------------------------------------------- |
| Setup endpoints  | Setup session token | `Authorization: Bearer <setup_session_token>` |
| User endpoints   | User session token  | `Authorization: Bearer <user_session_token>`  |
| Public endpoints | None                | —                                             |
| Runner endpoints | Runner token        | `Authorization: Bearer <runner_token>`        |

**Setup session tokens** are obtained by [verifying a bootstrap token](/reference/api/setup#verify-bootstrap-token). TTL: 30 minutes (sliding window).

**User session tokens** are obtained through the OIDC callback. TTL: 24 hours.

**Runner tokens** are obtained by registering a runner.

## Error format

All error responses use a consistent JSON structure:

```json
{
  "error": "Human-readable error message",
  "code": "machine_readable_error_code",
  "details": "Optional additional context"
}
```

The `details` field is omitted when not applicable.

### Common error codes

| HTTP Status | Code                          | Description                                  |
| ----------- | ----------------------------- | -------------------------------------------- |
| 400         | `invalid_input`               | Request body validation failed               |
| 400         | `invalid_redirect_uri`        | Redirect URI is malformed                    |
| 401         | `missing_auth`                | Authorization header not provided            |
| 401         | `invalid_session`             | Session token is invalid                     |
| 401         | `session_expired`             | Session token has expired                    |
| 401         | `no_session`                  | No active setup session exists               |
| 403         | `forbidden`                   | Insufficient RBAC permissions                |
| 403         | `user_not_found`              | No user account for this identity            |
| 409         | `already_configured`          | Setup is already complete (`ready` state)    |
| 409         | `invalid_state`               | Operation not valid in current setup state   |
| 409         | `setup_incomplete`            | Auth endpoints require setup to be complete  |
| 410         | `token_consumed`              | Bootstrap token already used                 |
| 410         | `token_expired`               | Bootstrap token TTL elapsed                  |
| 429         | `too_many_attempts`           | Rate limit exceeded (bootstrap verification) |
| 429         | `too_many_pending`            | Too many pending OIDC auth requests          |
| 500         | `store_error`                 | Database or storage error                    |
| 500         | `encryption_error`            | Failed to encrypt/decrypt secrets            |
| 502         | `oidc_discovery_error`        | Failed to discover OIDC provider             |
| 502         | `token_exchange_error`        | Failed to exchange authorization code        |
| 502         | `missing_id_token`            | IdP didn't return an ID token                |
| 502         | `id_token_verification_error` | ID token verification failed                 |
| 502         | `missing_email`               | ID token missing email claim                 |

## CORS

Default allowed origins:

- `http://localhost:3000`
- `http://127.0.0.1:3000`
- `http://localhost:4173`
- `http://127.0.0.1:4173`

Primary configuration lives in Settings API / Preferences UI. Environment
variables still work as fallback defaults:

- `OORE_CORS_ORIGINS` — comma-separated list (preferred)
- `OORE_CORS_ORIGIN` — single origin (backward compatible)

Allowed methods: `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `OPTIONS`

Allowed headers: `Content-Type`, `Authorization`

## Endpoint groups

### [Setup API](/reference/api/setup)

First-run instance configuration. Mutating endpoints require a setup session and are permanently disabled after `ready` state.

| Method | Path                               | Auth          |
| ------ | ---------------------------------- | ------------- |
| `GET`  | `/v1/public/setup-status`          | Public        |
| `POST` | `/v1/setup/bootstrap-token/verify` | Public        |
| `POST` | `/v1/setup/oidc/configure`         | Setup session |
| `POST` | `/v1/setup/owner/start-oidc`       | Setup session |
| `POST` | `/v1/setup/owner/verify-oidc`      | Setup session |
| `POST` | `/v1/setup/complete`               | Setup session |

### Auth API

OIDC authentication and session management. Only available when setup is complete.

| Method | Path                     | Auth         |
| ------ | ------------------------ | ------------ |
| `GET`  | `/v1/auth/oidc/start`    | Public       |
| `POST` | `/v1/auth/oidc/callback` | Public       |
| `POST` | `/v1/auth/logout`        | User session |

### Users API

User management. Requires a user session with appropriate RBAC permissions.

| Method   | Path                         | Auth                       |
| -------- | ---------------------------- | -------------------------- |
| `GET`    | `/v1/users/me`               | User session               |
| `GET`    | `/v1/users`                  | User session (owner/admin) |
| `POST`   | `/v1/users/invite`           | User session (owner/admin) |
| `PATCH`  | `/v1/users/{user_id}/role`   | User session (owner/admin) |
| `DELETE` | `/v1/users/{user_id}`        | User session (owner/admin) |
| `POST`   | `/v1/users/{user_id}/enable` | User session (owner/admin) |

### Projects API

Project management.

| Method   | Path                        | Auth                       |
| -------- | --------------------------- | -------------------------- |
| `GET`    | `/v1/projects`              | User session               |
| `POST`   | `/v1/projects`              | User session               |
| `GET`    | `/v1/projects/{project_id}` | User session               |
| `PATCH`  | `/v1/projects/{project_id}` | User session               |
| `DELETE` | `/v1/projects/{project_id}` | User session (owner/admin) |

### Pipelines API

Pipeline configuration and signing.

| Method   | Path                                  | Auth         |
| -------- | ------------------------------------- | ------------ |
| `GET`    | `/v1/projects/{project_id}/pipelines` | User session |
| `POST`   | `/v1/projects/{project_id}/pipelines` | User session |
| `GET`    | `/v1/pipelines/{pipeline_id}`         | User session |
| `PATCH`  | `/v1/pipelines/{pipeline_id}`         | User session |
| `DELETE` | `/v1/pipelines/{pipeline_id}`         | User session |
| `POST`   | `/v1/pipelines/validate`              | User session |

### Builds API

Build triggering and monitoring.

| Method | Path                               | Auth         |
| ------ | ---------------------------------- | ------------ |
| `POST` | `/v1/projects/{project_id}/builds` | User session |
| `GET`  | `/v1/builds`                       | User session |
| `GET`  | `/v1/builds/{build_id}`            | User session |
| `POST` | `/v1/builds/{build_id}/cancel`     | User session |

### Runners API

Runner registration and job management.

| Method  | Path                                | Auth         |
| ------- | ----------------------------------- | ------------ |
| `POST`  | `/v1/runners/register`              | User session |
| `GET`   | `/v1/runners`                       | User session |
| `PATCH` | `/v1/runners/{runner_id}`           | Runner token |
| `POST`  | `/v1/runners/{runner_id}/heartbeat` | Runner token |
| `POST`  | `/v1/runners/{runner_id}/claim`     | Runner token |

### Integrations API

Source control integrations (GitHub, GitLab).

| Method   | Path                                 | Auth         |
| -------- | ------------------------------------ | ------------ |
| `GET`    | `/v1/integrations`                   | User session |
| `GET`    | `/v1/integrations/{id}`              | User session |
| `DELETE` | `/v1/integrations/{id}`              | User session |
| `GET`    | `/v1/integrations/{id}/repositories` | User session |
| `POST`   | `/v1/integrations/github/start`      | User session |
| `POST`   | `/v1/integrations/github/complete`   | User session |
| `POST`   | `/v1/integrations/gitlab/start`      | User session |
| `POST`   | `/v1/integrations/gitlab/authorize`  | User session |

### Settings API

Instance configuration.

| Method | Path                            | Auth                       |
| ------ | ------------------------------- | -------------------------- |
| `GET`  | `/v1/settings/artifact-storage` | User session               |
| `PUT`  | `/v1/settings/artifact-storage` | User session (owner/admin) |
| `GET`  | `/v1/settings/preferences`      | User session               |
| `PUT`  | `/v1/settings/preferences`      | User session (owner/admin) |

### Build Logs API

Build log streaming and retrieval.

| Method | Path                                 | Auth               |
| ------ | ------------------------------------ | ------------------ |
| `GET`  | `/v1/builds/{build_id}/logs`         | User session       |
| `GET`  | `/v1/builds/{build_id}/logs/stream`  | Stream token (SSE) |
| `POST` | `/v1/builds/{build_id}/stream-token` | User session       |

### Artifacts API

Build artifact management and downloads.

| Method | Path                                        | Auth           |
| ------ | ------------------------------------------- | -------------- |
| `GET`  | `/v1/builds/{build_id}/artifacts`           | User session   |
| `POST` | `/v1/artifacts/{artifact_id}/download-link` | User session   |
| `GET`  | `/v1/artifacts/download/{token}`            | Download token |

### Webhooks

External webhook receivers (outside CORS).

| Method | Path                  | Auth             |
| ------ | --------------------- | ---------------- |
| `POST` | `/v1/webhooks/github` | GitHub signature |
| `POST` | `/v1/webhooks/gitlab` | GitLab token     |

### Health Check

| Method | Path       | Auth   |
| ------ | ---------- | ------ |
| `GET`  | `/healthz` | Public |

Returns `{"ok": true}`.

### Metrics

| Method | Path       | Auth   |
| ------ | ---------- | ------ |
| `GET`  | `/metrics` | Public |

Returns Prometheus-format metrics.
