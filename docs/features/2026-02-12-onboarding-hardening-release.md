# Onboarding Hardening for Public Launch

## Status

`ready`

## Problem

First-time onboarding still had high-friction failure modes:

- Hosted UI users saw generic fetch failures when backend connectivity was not viable.
- OIDC setup could strand operators after misconfiguration.
- Auth callback failures produced poor recovery guidance.
- macOS keychain prompts were surprising during first run.
- Several CLI commands looked implemented but were placeholders.
- Docs still implied hosted UI worked for localhost backends.
- Local-only operators lacked a production-ready local frontend path in release artifacts.

## User Impact

New operators now get deterministic recovery guidance instead of dead ends:

- Connectivity failures show explicit next actions (CLI, tunnel, local UI).
- OIDC setup can be corrected before owner verification is finalized.
- Callback errors route users back to the correct next step.
- Installer and docs set expectations for the keychain prompt.
- Installer now offers an explicit no-tunnel local web setup path (`oore-web`) including optional launch-at-login.
- Unimplemented CLI commands fail clearly with actionable workarounds.

## UI Changes

- `apps/web` setup/login now render connectivity-aware guidance for:
  - mixed-content (`https` frontend + `http` backend),
  - backend unreachable/network fetch failures.
- Add Instance now enforces hosted-origin guardrails:
  - hosted `ci.oore.build` requires explicit HTTPS backend URL,
  - rejects localhost HTTP and empty backend URL on hosted origin.
- Add Instance now enforces local-launcher guardrail:
  - on `oore-web` local origin, loopback backend URLs are rejected with guidance to leave URL empty (proxy mode).
- Setup Owner now provides recovery controls:
  - back to OIDC settings,
  - restart from token step.
- OIDC step copy now reflects true lifecycle behavior:
  - editable until owner verification is completed.
- Auth callback always renders actionable failure UI with flow-aware return CTA.
- Connectivity guidance now points local-only operators to `oore-web --backend-url <daemon-url>`.

## API Changes

- `POST /v1/setup/oidc/configure` is now accepted in both:
  - `bootstrap_pending`,
  - `idp_configured`.
- `POST /v1/setup/oidc/configure` remains blocked in:
  - `owner_created`,
  - `ready`.
- Reconfiguring OIDC now clears stale pending owner-auth state to prevent mismatch with prior client settings.
- No additional API surface changes in this increment.

## Security Considerations

- Mixed-content is explicitly blocked in UI before network call, reducing ambiguous failure handling.
- Hosted-origin backend URL validation reduces unsafe/invalid target entry.
- Clearing stale pending OIDC auth state on reconfigure prevents replay/mismatch against superseded client configuration.
- CLI messaging avoids false confidence for unimplemented admin/auth flows.

## Migration and Rollout

- No schema migration required.
- No command removals; hidden `oore setup open` alias is retained for compatibility.
- User-facing docs and setup copy standardize on `oore setup token`.
- Operators should ensure hosted usage points to HTTPS-reachable backend endpoints (direct or tunneled).
- Release tarballs now include:
  - `bin/oore-web` (precompiled local frontend launcher),
  - `web-dist/` static frontend assets consumed by `oore-web`.

## Acceptance Criteria

- [x] Hosted UI setup/login no longer fails as generic blank page for connectivity issues.
- [x] Setup surfaces explicit recovery paths for no-tunnel/local-only backend cases.
- [x] OIDC can be reconfigured in `idp_configured` and is still blocked in `owner_created`/`ready`.
- [x] Reconfiguring OIDC invalidates stale pending owner OIDC authorization state.
- [x] Auth callback failure states always render actionable CTA with diagnostic hint.
- [x] Installer/docs explain macOS keychain prompt and recommended action.
- [x] Installer asks localhost operators about internet exposure and offers local web launcher setup/launch-at-login.
- [x] CLI placeholder commands exit non-zero with workaround guidance.
- [x] API/docs reflect `/auth/callback` and updated OIDC configure state gate.

## Owner

Platform team

## Last Updated

`2026-02-12`
