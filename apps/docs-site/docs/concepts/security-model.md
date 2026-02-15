---
status: implemented
description: "Security architecture of oore.build including OIDC, RBAC, and encryption."
---

# Security Model

This page explains the security design decisions in oore.build and how they protect your instance.

## Authentication: OIDC (Remote) + Loopback Local Login (Local Only)

oore.build uses OpenID Connect (OIDC) for any non-loopback access (External Access / `runtime_mode=remote`). There are no local passwords, no password storage, and no password reset flows.

The daemon also supports loopback-only local login (`POST /v1/auth/local/login`) for local-first onboarding and local operator access. When setup is incomplete, local login is only available in Local Only mode; in Remote mode it is only available after setup is complete. In Local Only mode, the first successful local login may auto-complete setup and create the initial owner record.

**Why**: Eliminates an entire class of vulnerabilities (credential storage, brute force attacks, password reuse). Users authenticate with the same credentials they use for all other services. Disabling a user in your IdP immediately revokes their access.

### PKCE and CSRF protection

All OIDC flows use PKCE (S256 method) and CSRF state tokens:

- **PKCE**: Prevents authorization code interception attacks
- **State token**: Random value verified on callback to prevent CSRF
- **Nonce**: Included in the ID token to prevent replay attacks
- Pending auth entries expire after **10 minutes**

### Session management

- Session tokens are 32-byte random values generated with `OsRng`
- Only the SHA-256 hash is stored in the database
- User sessions have a **24-hour TTL**
- Setup sessions have a **30-minute sliding window TTL**
- Sessions are invalidated on logout or user disablement

## Authorization: RBAC

Four roles with fixed permission sets: owner, admin, developer, qa_viewer. Permissions are enforced at the API level — every request is checked before processing.

See [RBAC Reference](/reference/rbac) for the full permission matrix.

## Encryption at rest

Sensitive data (OIDC client secrets, signing credentials) is encrypted with **AES-256-GCM** before storage:

- The encryption key is stored in a file on disk at `<data-root>/encryption.key`
- Each encrypted value includes a unique nonce (IV)

## Bootstrap token security

The first-run bootstrap token (required for Remote-mode setup flows) has multiple protections:

| Protection | Detail |
|---|---|
| **Randomness** | 32 bytes (256 bits) from `OsRng` |
| **Storage** | Only SHA-256 hash stored; plaintext shown once |
| **TTL** | Configurable, default 15 minutes |
| **Single-use** | Consumed on first successful verification |
| **Rate limiting** | Locked after 5 failed attempts |

## Token and secret handling

| Data | Storage | Protection |
|---|---|---|
| Bootstrap token | SHA-256 hash in SQLite | Plaintext never stored |
| Setup session token | SHA-256 hash in SQLite | 30-minute sliding TTL |
| User session token | SHA-256 hash in SQLite | 24-hour TTL |
| OIDC client secret | AES-256-GCM encrypted in SQLite | File-stored encryption key |
| Signing certificates | AES-256-GCM encrypted in SQLite | Same encryption key |
| Keystore passwords | AES-256-GCM encrypted in SQLite | Same encryption key |

## CORS policy

The API restricts cross-origin requests:

- **Default origins**: `http://localhost:3000`, `http://127.0.0.1:3000`, `http://localhost:4173`, `http://127.0.0.1:4173`
- **Configuration**: Stored in SQLite (Preferences UI); env fallback via `OORE_CORS_ORIGINS` / `OORE_CORS_ORIGIN`
- **Methods**: GET, POST, PUT, PATCH, DELETE, OPTIONS
- **Headers**: Content-Type, Authorization

## Network model

oore.build is designed for local-network deployment:

- The daemon listens on `127.0.0.1:8787` by default (localhost only)
- For remote access, place the daemon behind a reverse proxy with TLS
- Runner-to-daemon communication happens over your local network
- The hosted UI at `ci.oore.build` connects directly to your daemon — no data proxied through a third party

## Audit logging

User management operations are logged:

- `user_invited`, `role_changed`, `user_disabled`, `user_enabled`, `user_activated`, `owner_created`

These events are stored in the SQLite database and available through the admin interface.
