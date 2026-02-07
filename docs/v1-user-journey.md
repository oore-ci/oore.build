# V1 User Journey (Execution-First)

Status: Active guidance for product, API, and UI completion checks.
Last updated: 2026-02-07

## Purpose

This document is the V1 guiding flow for implementation order and "done" checks.
No build-related UI task is complete unless the relevant journey checkpoint here is satisfied.

## Scope and Constraints

- Applies to self-hosted customer backend on macOS (V1 contract).
- Hosted `ci.oore.build` is UI-only and does not run customer builds.
- Auth is OIDC-only.
- Frontend and backend remain strictly separated.

## Canonical User Journey

1. Instance bootstrap and access
2. Git provider connection
3. Project creation from connected repository
4. Pipeline configuration
5. Trigger policy configuration
6. Build trigger and planning
7. Runner claim and execution
8. Live logs and build status
9. Artifact publishing and download
10. Operator and team follow-up actions (cancel/retry/audit)

## Journey Checkpoints (Definition of Done)

### 1) Instance bootstrap and access

Actor: Owner/Admin operator

User action:
- Complete setup flow and sign in via OIDC.

System outcome:
- Instance reaches `ready`.
- Owner/Admin can access dashboard.

Done when:
- `GET /v1/public/setup-status` is non-sensitive and accurate.
- Setup mutating endpoints are disabled after `ready`.

### 2) Git provider connection

Actor: Owner/Admin

User action:
- Open Integrations, select provider, authorize app/OAuth installation, pick repos/org scope.

System outcome:
- Provider installation linkage is stored for the instance.
- Webhook signature secret and provider tokens are stored encrypted.

Done when:
- UI shows provider connection status.
- Disconnect/reconnect path is available and audited.
- Provider auth data never appears in public endpoints or logs.

### 3) Project creation from repository

Actor: Developer/Admin

User action:
- Create project from a connected repository and choose default branch.

System outcome:
- Project is linked to provider/repository identity.
- RBAC and audit trail are enforced.

Done when:
- Project create/list/detail/update/delete behavior is role-gated.
- Invalid or unauthorized repo selections are blocked.

### 4) Pipeline configuration

Actor: Developer/Admin

User action:
- Configure pipeline definition (repo-backed config file is source of truth for execution).

System outcome:
- System can resolve pipeline config at a specific commit SHA.
- Validation blocks invalid trigger definitions/required inputs.

Done when:
- Pipeline validation errors are actionable in UI.
- Execution uses snapshot-at-trigger config, not mutable live draft.

### 5) Trigger policy configuration

Actor: Developer/Admin

User action:
- Enable manual/API/webhook triggers and choose concurrency behavior (for example: cancel previous on same branch).

System outcome:
- Trigger sources and concurrency policy are persisted and enforced.

Done when:
- Stale-build cancellation behavior is deterministic and testable.
- Trigger events are auditable with actor/source metadata.

### 6) Build trigger and planning

Actor: Developer/Automation webhook

User action:
- Trigger build manually or via git event.

System outcome:
- Immutable build record is created with commit SHA, resolved config, and initial queued state.

Done when:
- Build lifecycle states are validated server-side.
- Cancel endpoint works from queued/running with valid terminal transitions.

### 7) Runner claim and execution

Actor: Runner + operator

User action:
- Registered runner heartbeats and claims queued work.

System outcome:
- Single runner claim via lease/lock semantics.
- Build executes in isolated ephemeral workspace.

Done when:
- No duplicate claims for same job.
- Timeout/cancel cleanly stops execution and updates terminal state.

### 8) Live logs and build status

Actor: Developer/QA/Admin

User action:
- Open build detail and watch execution progress.

System outcome:
- Live logs stream in order.
- State transitions and timings are visible.

Done when:
- SSE log stream is stable during long-running builds.
- UI handles reconnect/history without losing context.

### 9) Artifact publishing and download

Actor: Developer/QA/Admin

User action:
- Download/install generated artifacts.

System outcome:
- Runner uploads artifact metadata and binaries.
- API returns short-lived signed download links with RBAC checks.

Done when:
- Artifact list shows type, size, checksum, and timestamp.
- Expired links fail safely and are re-issuable by authorized users.

### 10) Operator and team follow-up

Actor: Owner/Admin/Developer

User action:
- Cancel or rerun builds, inspect audit events, check runner health.

System outcome:
- Operational controls are available through UI and `oore` CLI.
- Security and operational audits remain traceable.

Done when:
- Action history is attributable (who, what, when, source).
- Core operator actions have CLI parity (`oore status`, `oore runner register`, etc.).

## UI Completion Gate (Mandatory)

Before marking any UI task complete, confirm:

- The UI step maps to one checkpoint in this document.
- API behavior required by that checkpoint exists and is role-gated.
- At least one happy-path and one failure-path scenario for that checkpoint was manually tested.
- Audit and security expectations for that checkpoint are satisfied.
- Related feature docs were updated under `docs/features/` if user-facing behavior changed.

If any item above is not met, the task remains in-progress.

## Priority Mapping

- `P0` checkpoints: 2 through 9 (end-to-end build loop)
- `P1` checkpoints: 3 through 5 UX hardening and operator CLI parity in 10
- `P2` checkpoints: advanced reliability (rerun policies, approvals, expanded E2E depth)

## Relationship to Roadmap

- Roadmap sequencing lives in `docs/v1-roadmap.md`.
- This document defines user-visible flow correctness and completion gates.
- If roadmap tasks conflict with this journey, fix the roadmap sequence before implementation.
