# OIDC Auth and Session Management

## Status

`ready`

## Problem

After an oore.build instance completes setup, users need a way to authenticate against the configured OIDC identity provider and maintain sessions. The platform requires OIDC-only authentication (no local passwords) with PKCE-based authorization code flow, in-memory session management with hashed tokens, and explicit logout.

## User Impact

Authenticated users can sign in through their organization's OIDC provider (e.g., Google, Okta, Auth0). After completing the OIDC flow, users receive a session token that is valid for 24 hours. Sessions can be explicitly revoked via logout. The auth flow is only available after setup is complete (`setup_state == Ready`).

## UI Changes

The frontend login flow consumes the OIDC start/callback endpoints programmatically. On sign-out, the frontend now clears local auth state and immediately redirects to `/login` without requiring a manual browser refresh. The `/login` view now includes a saved-instance switcher and an "Add Another Instance" action so operators can switch targets while logged out. The UI also surfaces the last successful sign-in timestamp on-device and confirms OIDC as the active sign-in method.

## API Changes

- `GET /v1/auth/oidc/start?redirect_uri=<optional>` -- Initiates the OIDC authorization code flow with PKCE. Performs provider discovery, generates a PKCE challenge (S256), builds the authorization URL with `openid`, `email`, and `profile` scopes, and stores the pending auth request keyed by CSRF state token. Returns `{ "authorization_url": "...", "state": "..." }`. Only available when `setup_state == Ready`.
- `GET /v1/auth/oidc/callback?code=<code>&state=<state>` -- Handles the IdP callback. Validates the CSRF state parameter, checks pending auth expiry (10-minute TTL), exchanges the authorization code for tokens using the PKCE verifier, verifies the ID token signature and claims (including nonce), extracts `email` and `subject` from claims, creates a session (24-hour TTL), and returns `{ "session_token": "...", "expires_at": <epoch>, "user": { "email": "...", "oidc_subject": "..." } }`.
- `POST /v1/auth/logout` -- Requires Bearer session token. Validates the session, then revokes it. Returns `{ "ok": true }`. Returns 401 if the token is missing or invalid.

## Security Considerations

- **PKCE (S256)**: Every authorization request generates a fresh random PKCE challenge and verifier. The verifier is stored server-side and never exposed to the client.
- **CSRF protection**: A random CSRF state token is generated per auth request and validated on callback. Unknown or expired state values are rejected.
- **Pending auth TTL**: Pending OIDC authorization requests expire after 10 minutes. Expired entries are cleaned up on each new `/start` call.
- **Session token hashing**: Session tokens are generated with cryptographic randomness (`generate_session_token`), hashed with SHA-256 before storage. Plaintext tokens are never persisted.
- **Session TTL**: Sessions expire after 24 hours (86,400 seconds). Expired sessions are rejected on validation.
- **In-memory session store**: Sessions are stored in a `HashMap` behind a `Mutex`. Sessions do not survive daemon restarts (by design for V1).
- **No-redirect HTTP client**: The HTTP client used for OIDC discovery and token exchange is configured with `redirect::Policy::none()` to prevent SSRF via open redirects.
- **ID token verification**: The ID token signature is verified against the provider's JWKS, and claims (including nonce) are validated before extracting user identity.
- **Auth endpoints gated on setup state**: All auth endpoints return 409 if `setup_state != Ready`, preventing auth flows before OIDC is properly configured.

## Migration and Rollout

First implementation. Sessions are in-memory only and do not persist across daemon restarts. No migration needed.

## Acceptance Criteria

- [x] `GET /v1/auth/oidc/start` returns a valid authorization URL with PKCE challenge
- [x] CSRF state parameter is validated on callback
- [x] Pending auth requests expire after 10 minutes
- [x] Authorization code is exchanged with PKCE verifier
- [x] ID token signature and nonce are verified
- [x] Email claim is extracted and required
- [x] Session is created with 24-hour TTL on successful callback
- [x] Session tokens are hashed before storage
- [x] `POST /v1/auth/logout` revokes the session
- [x] Frontend sign-out transitions to `/login` immediately after logout state is cleared
- [x] Logged-out `/login` view supports switching saved instances and adding another instance
- [x] Login view shows the last successful sign-in timestamp (local device metadata) and OIDC method
- [x] All auth endpoints return 409 when setup is not complete
- [x] HTTP client prevents redirect-based SSRF
- [x] OIDC discovery module validates required endpoints (authorization, token, JWKS)
- [x] Feature documentation passes docs gate

## Owner

Platform team

## Last Updated

`2026-02-11`
