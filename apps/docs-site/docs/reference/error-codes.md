---
status: implemented
description: 'Complete list of machine-readable error codes returned by the Oore CI API.'
---

# Error Codes

All error codes returned by the Oore CI API, consolidated from all endpoint groups.

## Error format

```json
{
  "error": "Human-readable error message",
  "code": "machine_readable_error_code",
  "details": "Optional additional context"
}
```

## Authentication errors

| Code              | HTTP Status | Description                             |
| ----------------- | ----------- | --------------------------------------- |
| `missing_auth`    | 401         | Authorization header not provided       |
| `invalid_session` | 401         | Session token is invalid                |
| `session_expired` | 401         | Session token has expired               |
| `no_session`      | 401         | No active setup session exists          |
| `forbidden`       | 403         | Insufficient RBAC permissions           |
| `user_not_found`  | 403         | No user account for this email/identity |

## Setup errors

| Code                 | HTTP Status | Description                                  |
| -------------------- | ----------- | -------------------------------------------- |
| `already_configured` | 409         | Setup is already complete (state is `ready`) |
| `invalid_state`      | 409         | Operation not valid in current setup state   |
| `setup_incomplete`   | 409         | Auth endpoints require setup to be complete  |
| `invalid_token`      | 401         | Bootstrap token hash does not match          |
| `token_consumed`     | 410         | Bootstrap token already used                 |
| `token_expired`      | 410         | Bootstrap token TTL elapsed                  |
| `no_bootstrap_token` | 500         | No bootstrap token has been generated        |
| `too_many_attempts`  | 429         | 5+ failed bootstrap verification attempts    |

## OIDC errors

| Code                          | HTTP Status | Description                                            |
| ----------------------------- | ----------- | ------------------------------------------------------ |
| `oidc_discovery_failed`       | 400         | Could not fetch OIDC discovery document (during setup) |
| `oidc_discovery_error`        | 502         | OIDC discovery HTTP request failed (during auth)       |
| `oidc_not_configured`         | 500         | OIDC configuration is missing                          |
| `oidc_config_error`           | 500         | OIDC configuration is invalid                          |
| `auth_expired`                | 400         | OIDC auth request expired (10-minute TTL)              |
| `token_exchange_error`        | 502         | Failed to exchange authorization code                  |
| `missing_id_token`            | 502         | IdP didn't return an ID token                          |
| `id_token_verification_error` | 502         | ID token signature/claims verification failed          |
| `missing_email`               | 502         | ID token missing email claim                           |
| `too_many_pending`            | 429         | Too many pending auth requests (limit: 1000)           |

## Input validation errors

| Code                   | HTTP Status | Description                    |
| ---------------------- | ----------- | ------------------------------ |
| `invalid_input`        | 400         | Request body validation failed |
| `invalid_redirect_uri` | 400         | Redirect URI is malformed      |

## Storage and system errors

| Code                | HTTP Status | Description                        |
| ------------------- | ----------- | ---------------------------------- |
| `store_error`       | 500         | Database operation failed          |
| `encryption_error`  | 500         | Failed to encrypt secrets          |
| `decryption_error`  | 500         | Failed to decrypt stored secrets   |
| `http_client_error` | 500         | Failed to create HTTP client       |
| `session_error`     | 500         | Session creation/validation failed |
