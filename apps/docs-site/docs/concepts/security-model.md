---
status: implemented
description: 'Security architecture of Oore CI including Remote auth providers, RBAC, and encryption.'
---

# Security Model

This page explains the security design decisions in Oore CI and how they protect your instance.

## Authentication: Remote OIDC or Trusted Proxy + Host-Authorized Recovery

For any non-loopback access (`runtime_mode=remote`), Oore CI requires one of:

- OpenID Connect (OIDC)
- Trusted Proxy mode (for example Warpgate or an identity-aware proxy)

There are no local passwords, no password storage, and no password reset flows.

The daemon supports loopback-only local login (`POST /v1/auth/local/login`) for Local Only onboarding and operator access. The first successful Local Only login may auto-complete setup and create the initial owner record.

In a Ready Remote instance, TCP loopback is not authentication authority. A local operator runs [`oore recovery`](/reference/cli/oore-recovery), which connects through an owner-only Unix management socket and mints a short-lived, single-use capability bound to one active account. The browser receives that capability in a URL fragment, removes the fragment from browser history, and submits it in the local-login POST body. The daemon atomically consumes the capability before creating a session. Missing, expired, replayed, malformed, unknown, or wrong-account capabilities fail regardless of loopback or forwarding headers.

**Why**: Eliminates an entire class of vulnerabilities (credential storage, brute force attacks, password reuse). Users authenticate with identity systems already used by the organization. Disabling a user in the upstream identity system revokes their ability to start new sessions.

### Trusted Proxy mode

In Trusted Proxy mode, Oore CI trusts identity headers from an upstream proxy and creates normal Oore sessions per user.

- Default identity header: `x-oore-user-email` (expected to be an email)
- Setup UI presets can switch the header to provider-specific defaults such as `x-warpgate-username`
- Shared-secret header for protected proxy hops: `x-oore-trusted-proxy-secret`
- Trust boundary: headers are accepted only when the immediate peer is trusted (loopback by default, optional CIDR allowlist for remote proxy peers)
- Authorization stays in Oore RBAC (owner/admin/developer/qa_viewer) via Oore users and roles

This mode does not introduce local passwords; it shifts authentication to the upstream access proxy while preserving Oore sessioning, RBAC, and audit attribution.

When `oore-web` sits between the browser and backend, it treats browser-supplied identity headers as untrusted. It strips common identity headers unless the upstream auth proxy also sends an `oore-web` proof header, then it forwards the identity header and injects the backend shared secret on the proxied API request.

### PKCE and CSRF protection (OIDC mode)

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

| Protection        | Detail                                         |
| ----------------- | ---------------------------------------------- |
| **Randomness**    | 32 bytes (256 bits) from `OsRng`               |
| **Storage**       | Only SHA-256 hash stored; plaintext shown once |
| **TTL**           | Configurable, default 15 minutes               |
| **Single-use**    | Consumed on first successful verification      |
| **Rate limiting** | Locked after 5 failed attempts                 |

## Token and secret handling

| Data                        | Storage                         | Protection                 |
| --------------------------- | ------------------------------- | -------------------------- |
| Bootstrap token             | SHA-256 hash in SQLite          | Plaintext never stored     |
| Setup session token         | SHA-256 hash in SQLite          | 30-minute sliding TTL      |
| User session token          | SHA-256 hash in SQLite          | 24-hour TTL                |
| Local recovery capability   | SHA-256 hash in daemon memory   | At most 5 minutes, one use |
| OIDC client secret          | AES-256-GCM encrypted in SQLite | File-stored encryption key |
| Trusted proxy shared secret | AES-256-GCM encrypted in SQLite | File-stored encryption key |
| Signing certificates        | AES-256-GCM encrypted in SQLite | Same encryption key        |
| Keystore passwords          | AES-256-GCM encrypted in SQLite | Same encryption key        |

## CORS policy

The API restricts cross-origin requests:

- **Default origins**: `http://localhost:3000`, `http://127.0.0.1:3000`, `http://localhost:4173`, `http://127.0.0.1:4173`
- **Configuration**: Stored in SQLite (Preferences UI); env fallback via `OORE_CORS_ORIGINS` / `OORE_CORS_ORIGIN`
- **Methods**: GET, POST, PUT, PATCH, DELETE, OPTIONS
- **Headers**: Content-Type, Authorization

## Network model

Oore CI is designed for local-network deployment:

- The daemon listens on `127.0.0.1:8787` by default (localhost only)
- For remote access, place the daemon behind a reverse proxy with TLS
- Runner-to-daemon communication happens over your local network
- The hosted UI at `ci.oore.build` connects directly to your daemon — no data proxied through a third party

## Repository execution trust

Oore V1 uses a **Direct macOS runner** for repositories you trust. Checkout and build commands run with the permissions of the runner's macOS account; Oore does not use `sandbox-exec` or claim that Direct mode contains hostile repository code.

Only an Owner or Admin may create a project or change its linked source
repository. That link is the explicit trust decision: the operator accepts the
same code, dependency, build-script, and contributor risk as running the project
directly on the runner Mac. Oore does not add a second repository execution
allowlist. External-fork pull and merge requests are ignored rather than executed
automatically.

Every build snapshot records the exact repository link it was created from.
Runners claim queued work only while that identity still matches the project;
changing a project source cancels queued snapshots from the previous repository.
Already-active work keeps its build-bound checkout identity so an operator or
upgrade can drain it without silently changing the code being built.

**Accept new builds** in instance Preferences is an operational pause, not a
trust grant. Pausing lets active work finish and prevents runners from claiming
queued work until an Owner or Admin resumes it.

Private workspaces, environment scrubbing, scoped checkout credentials, late signing-material retrieval, one-time signing grants, fixed signer commands, output verification, and cleanup remain defense-in-depth against accidental leakage. iOS profiles stay in the private job workspace and signing uses an explicit temporary keychain without changing the runner user's default keychain, search list, or globally installed profiles. These controls are not an isolation boundary against malicious code already trusted to run under the same account.

A dedicated non-admin runner account is recommended. Strong isolation for untrusted contributions requires disposable macOS virtual machines and is outside V1.

## Audit logging

User management and local recovery operations are logged:

- `user_invited`, `role_changed`, `user_disabled`, `user_enabled`, `user_activated`, `owner_created`
- `local_recovery_capability_minted`, `local_recovery_capability_mint_failed`, `local_recovery_login_succeeded`, `local_recovery_login_failed`

Recovery audit details contain only bounded metadata such as a non-secret capability ID, reason, expiry, and transport facts. Raw capabilities are never written to audit details.
