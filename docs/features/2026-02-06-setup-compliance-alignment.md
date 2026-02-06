# Setup Compliance Alignment

## Status

`ready`

## Problem

Several implementation details drifted from the documented security contracts and platform conventions during initial development. Specifically: (1) the OIDC HTTP client used `redirect::Policy::limited(5)` instead of the documented `redirect::Policy::none()`, weakening SSRF protections; (2) the owner creation API endpoint path did not match the platform contract; (3) dead code accumulated across backend modules; and (4) no automated test baseline existed for the backend crates. These gaps needed to be closed before the V1 platform can be considered contract-compliant.

## User Impact

End users are not directly affected by UI changes. Operators benefit from stronger SSRF protections on the OIDC HTTP client (no redirects followed during discovery or token exchange), a contract-aligned setup API surface, and a cleaner codebase with fewer maintenance risks.

## UI Changes

No UI changes. All fixes are backend-only (Rust crates and documentation).

## API Changes

- `POST /v1/setup/owner/finalize` removed from contract. Replaced by `POST /v1/setup/owner/start-oidc` and `POST /v1/setup/owner/verify-oidc` to match actual OIDC-based owner creation flow.
- No new endpoints added. No payload or response schema changes.

## Security Considerations

- **SSRF prevention via no-redirect policy**: The `reqwest` HTTP client used for OIDC discovery and token exchange is now configured with `redirect::Policy::none()`. This prevents server-side request forgery attacks where a malicious or compromised OIDC provider could redirect the daemon's outbound HTTP requests to internal network addresses. OIDC discovery and token exchange endpoints are well-known stable URLs that should never issue redirects under normal operation.
- **Dead code removal**: Unused functions and imports were removed to reduce attack surface and improve auditability.
- **No new secrets or credentials introduced**.

## Migration and Rollout

No migration required. The redirect policy change is backward-compatible because compliant OIDC providers do not redirect on discovery or token exchange endpoints. The owner endpoint path change aligns the implementation with the already-published contract, so any frontend consumers already targeting the contract path will work correctly.

## Acceptance Criteria

- [x] `build_http_client()` in `auth.rs` uses `redirect::Policy::none()`
- [x] Owner creation endpoint matches `platform-contract.md` path
- [x] No dead code warnings from `cargo check --workspace`
- [x] `cargo check --workspace` compiles successfully
- [x] Feature documentation passes `scripts/check-feature-docs.sh` gate

## Owner

Platform team

## Last Updated

`2026-02-06`
