# SCM Integration Blueprint (V1)

Status: Active implementation guidance  
Last updated: 2026-02-07

## Why this exists

This is the execution guide for connecting GitHub and GitLab in V1 without breaking platform constraints:

- Hosted `ci.oore.build` is UI-only.
- Customer backend is the control plane and webhook receiver.
- Integrations must support multiple connections with no hard product-imposed cap.

## Core Decision

Use **backend-owned integration endpoints** in all deployment modes.

- Hosted UI mode: `ci.oore.build` starts the flow, but callback and webhook endpoints are on the customer backend.
- Fully self-hosted mode: UI + backend are customer-hosted, and callbacks/webhooks terminate there.

Do **not** rely on a single oore-managed global GitHub App in V1, because GitHub App webhook config is app-level (single webhook endpoint per app), while V1 requires direct delivery to each customer backend and no hosted webhook control plane.

## GitHub (V1 Standard)

### Recommended auth model

- Use **GitHub Apps**, not OAuth apps, for repo-scoped automation.
- Per backend instance, use one or more instance-owned apps ("BYO app"), installed on target orgs/repos.

### Onboarding flow

1. Owner/Admin clicks `Connect GitHub`.
2. Backend provides either:
- GitHub App manifest registration flow, or
- Pre-filled app registration URL parameters.
3. User creates/owns the app (personal/org-owned).
4. User installs app to org/account and selects repos.
5. Backend stores app metadata, installation IDs, and encrypted secrets.
6. GitHub sends webhook events directly to customer backend webhook endpoint.

### Why this is reliable

- Installation model is first-class and supports multiple installs across accounts/orgs.
- Webhook signature verification and delivery IDs allow secure idempotent handling.
- Missed deliveries can be redelivered explicitly via GitHub tooling/API.

## GitLab (V1 Standard)

### Recommended auth model

- Support `gitlab.com` and self-managed GitLab by storing provider base URL per integration.
- Primary mode: OAuth application per GitLab host + project/group webhooks.
- Fallback mode (if OAuth app creation is blocked): scoped token + webhooks (documented as lower-security convenience mode).

### Onboarding flow

1. Owner/Admin clicks `Connect GitLab`.
2. User selects host:
- `https://gitlab.com`, or
- `https://<self-managed-host>`.
3. User registers OAuth app on that host (or provides token in fallback mode).
4. User configures project/group webhook URL + secret token to customer backend.
5. Backend verifies webhook secret/token and stores integration metadata.

### Why this is reliable

- Same endpoint model for GitLab.com and self-managed; only base URL changes.
- GitLab webhook history + resend supports operational recovery.
- Group webhooks can reduce per-project setup where plan/tier permits.

## Multi-Integration Model (No Hard Limit)

V1 should allow unlimited integrations from product perspective.  
Enforce only practical safeguards (rate limits, UI pagination, queue pressure), not arbitrary count caps.

Minimum model:

- `integrations`: one row per provider connection (provider, host, display name, auth mode, status)
- `integration_credentials`: encrypted secrets/token material with rotation metadata
- `integration_installations`: provider installation/group/project linkage
- `integration_repositories`: mapped repos/projects for trigger scope
- `integration_webhooks`: endpoint status, last delivery timestamp, last failure reason

Rules:

- One backend instance can have many integrations across GitHub and GitLab.
- One integration can map to many installations/repos/projects.
- Build triggers must resolve the exact integration + installation context per project.

## Webhook Reliability Requirements (Mandatory)

- Verify provider signature/token on every webhook.
- Reject stale or replayed deliveries using event IDs + replay window.
- ACK fast (`2xx`), enqueue async processing.
- Store raw payload + normalized event metadata for audit/debug (with secret scrubbing).
- Idempotent processing by provider delivery ID.
- Provide operator tools:
- last delivery status
- failure reason
- resend/reconcile guidance

## Hosted UI vs Self-Hosted UI Summary

- **Same backend contract in both modes.**  
Only the frontend origin changes.
- Hosted UI does not act as webhook relay/proxy in V1.
- CORS and allowed origins must include the selected UI origin(s), but SCM callbacks/webhooks remain backend endpoints.

## Priority in Roadmap

This architecture is reflected in `docs/v1-roadmap.md`:

- Phase 2 (`P0`) includes SCM integration schema, GitHub flow, GitLab flow, and webhook hardening before trigger reliability is considered complete.

## References

- GitHub Apps and webhooks:
- https://docs.github.com/en/apps/creating-github-apps/registering-a-github-app/using-webhooks-with-github-apps
- https://docs.github.com/en/apps/sharing-github-apps/registering-a-github-app-from-a-manifest
- https://docs.github.com/webhooks/using-webhooks/best-practices-for-using-webhooks
- https://docs.github.com/en/webhooks/using-webhooks/handling-failed-webhook-deliveries
- https://docs.github.com/en/developers/webhooks-and-events/webhooks/securing-your-webhooks

- GitLab webhooks and OAuth:
- https://docs.gitlab.com/user/project/integrations/webhooks/
- https://docs.gitlab.com/api/project_webhooks/
- https://docs.gitlab.com/api/group_webhooks/
- https://docs.gitlab.com/api/oauth2/
- https://docs.gitlab.com/integration/gitlab/
