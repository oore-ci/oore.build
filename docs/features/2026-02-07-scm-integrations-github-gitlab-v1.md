# SCM Integrations for V1 (GitHub + GitLab)

## Status

`ready`

## Problem

The integration flow for source providers was not clearly defined across deployment modes.  
Without a strict pattern, webhook delivery, token handling, and trigger reliability become inconsistent, especially when using hosted UI (`ci.oore.build`) with customer-owned backends.

## User Impact

Owners/Admins get a reliable and predictable onboarding path for GitHub and GitLab:

- They can connect providers regardless of whether UI is hosted or self-hosted.
- Webhooks trigger builds reliably because delivery targets the customer backend.
- They can add multiple integrations (multiple accounts/groups/providers) without artificial limits.

## UI Changes

Add or refine an Integrations UX with:

- Provider cards (`GitHub`, `GitLab`)
- "Add integration" flow
- Connection status, scope summary, and last webhook receipt time
- Disconnect/reconnect actions with confirmation
- Per-integration repository selection where provider supports it

Flow behavior by deployment mode:

- Hosted UI (`ci.oore.build`): user starts from hosted frontend, but callback and webhook endpoints are customer-backend endpoints.
- Self-hosted UI: user starts and finishes within self-hosted UI/backend; callback and webhooks terminate on the same backend.

## API Changes

Planned API surface (names may be finalized during implementation):

- `POST /v1/integrations/github/start`
- `POST /v1/integrations/github/complete`
- `POST /v1/integrations/gitlab/start`
- `POST /v1/integrations/gitlab/complete`
- `GET /v1/integrations`
- `DELETE /v1/integrations/{integration_id}`
- Provider webhook ingress endpoints under `/v1/webhooks/{provider}`

Provider strategy:

- GitHub: V1 uses GitHub Apps (instance-owned/BYO App), then app installation per account/org/repo scope.
- GitLab: V1 uses OAuth application or token-based integration (instance URL aware for `gitlab.com` vs self-managed) plus project/group webhooks.

## Security Considerations

- Encrypt all provider secrets/tokens at rest.
- Verify webhook authenticity:
- GitHub: `X-Hub-Signature-256` HMAC verification.
- GitLab: shared secret token verification (`X-Gitlab-Token`) and source checks.
- Enforce idempotency on webhook event IDs to prevent duplicate-trigger races.
- Store minimal scopes and rotate short-lived tokens where supported.
- Keep callbacks and webhook endpoints backend-owned; hosted UI must not become a webhook proxy in V1.

## Migration and Rollout

- No breaking migration for existing setup/auth flows.
- Implement integrations as a new capability gated to `owner/admin`.
- Rollout order:
1. Integration schema and encrypted secret store
2. GitHub App flow
3. GitLab flow (`gitlab.com` + self-managed base URL support)
4. Webhook hardening + trigger pipeline hookup

## Acceptance Criteria

- [x] V1 integration architecture is documented for hosted-UI and self-hosted-UI modes.
- [x] GitHub and GitLab onboarding flows are defined with webhook delivery rules.
- [x] Multi-integration support is explicitly required (no fixed cap in model/UX/API).
- [x] Roadmap priority includes SCM integration as `P0` prerequisite to trigger reliability.

## Owner

Platform Team

## Last Updated

`2026-02-07`
