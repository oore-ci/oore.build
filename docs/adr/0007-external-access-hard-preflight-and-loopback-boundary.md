# ADR-0007: External Access Hard Preflight and Loopback-Only Local Auth

## Status

Accepted

## Date

2026-02-14

## Context

The local-first alpha flow made it easy to operate on a single machine, but
the security boundary for non-loopback access was not strict enough.

Two concrete risks existed:

1. `local` auth could be attempted from non-loopback clients when the daemon
   was reachable on LAN/Tailscale/`.local`.
2. Enabling `remote` mode lacked a hard fail-closed preflight gate to ensure
   HTTPS/public URL/origin policy and OIDC readiness were valid before exposure.

## Decision

1. `POST /v1/auth/local/login` is loopback-only.
2. Any non-loopback usage path requires explicit External Access enablement
   (`runtime_mode=remote`) and OIDC auth.
3. Enabling External Access is owner-only and guarded by hard preflight checks:
   - setup state is `ready`
   - OIDC config is valid for runtime auth
   - `OORE_PUBLIC_URL` exists, is non-loopback, and uses `https`
   - public origin is allowlisted in configured CORS origins
   - redirect/origin policy consistency check passes
4. Runtime mode changes revoke all active sessions.

## Rationale

### Preserve frictionless local onboarding

Loopback still supports one-click local auth and local-first setup.

### Fail closed for network paths

Non-loopback access now requires explicit operator intent, valid OIDC, and a
secure transport/origin posture.

### Keep rollout practical

Preflight blocks only core security prerequisites; provider/webhook readiness
remains advisory for this increment.

## Consequences

- Local auth is no longer possible over LAN/Tailscale/non-loopback hosts.
- External Access enablement returns deterministic failure codes for blocked
  preflight conditions.
- Role policy is tightened for runtime mode mutation (owner-only).
- Existing sessions are invalidated on mode switches, forcing re-auth under
  the active mode policy.

## Contract References

- `docs/platform-contract.md` sections 7, 13, 14
- `docs/strict-guidelines.md` bootstrap and security rules
