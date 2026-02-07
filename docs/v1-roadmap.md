# V1 Implementation Roadmap

Status: Active - execution-first sequencing for V1 CI completion.
Last assessed: 2026-02-07

## Why This Revision

- Previous sequencing over-weighted project/pipeline CRUD before a complete CI execution loop.
- Industry CI/CD systems (Codemagic, GitHub Actions, Buildkite, CircleCI, GitLab CI) prioritize: trigger -> plan -> queue -> claim -> execute -> logs/artifacts -> completion.
- This roadmap now follows that flow and marks every remaining task with explicit priority.

## Priority Legend

- `P0` = critical path for a reliable end-to-end CI build loop (must complete first)
- `P1` = important operator/product usability once `P0` works
- `P2` = hardening, scale, polish, and release confidence

## Reference CI Flow (Normalized)

1. Receive trigger (push/PR/tag/manual/API)
2. Resolve config/workflow at commit SHA
3. Create immutable build record + plan
4. Enqueue and schedule (with concurrency/cancel policies)
5. Match runner and claim work
6. Execute in isolated workspace
7. Stream logs and status transitions
8. Persist artifacts and publish download links
9. Support cancel/retry/rerun and audit trail

## Journey Gate (Mandatory)

- Use `docs/v1-user-journey.md` as the release gate for build-related product work.
- A task is not complete until its corresponding journey checkpoint is satisfied (including failure-path behavior).
- If roadmap ordering conflicts with the journey, update roadmap sequencing before implementation continues.

## Foundation (Complete)

These are done and passing `make validate`:

- [x] Setup bootstrap state machine (backend + CLI)
- [x] Setup wizard UI (4-step frontend)
- [x] OIDC auth with PKCE + session management
- [x] Multi-instance frontend with setup session isolation
- [x] SQLite store with embedded migrations (setup tables)
- [x] AES-256 secrets-at-rest encryption
- [x] OpenTelemetry tracing + Prometheus metrics
- [x] shadcn/Base UI component system
- [x] TanStack Router file-based routing
- [x] VitePress documentation site
- [x] Governance docs (contract, guidelines, policy, agents)
- [x] 14 feature docs, 3 ADRs
- [x] Makefile targets and `make validate` gate

## Phase 1: Identity + RBAC Core (Complete)

- [x] Expand SQLite schema (`users`, `sessions`, `audit_logs`)
- [x] Wire Casbin RBAC middleware in Axum
- [x] User management endpoints (invite, role update, disable/enable)
- [x] Persistent sessions with restart-safe validation
- [x] OIDC login/callback flow with invite activation rules
- [x] User management UI and role-gated navigation
- [x] Audit logging for privileged actions

Feature docs: `2026-02-06-rbac-and-user-management.md`, `2026-02-06-session-persistence.md`

## Phase 1.5: Design System + App Shell (Complete)

- [x] `DESIGN.md` governance and shadcn-first policy
- [x] Migration to shadcn/Base UI primitives
- [x] Sidebar app shell with instance switcher
- [x] Instance icon picker and edit dialog UX
- [x] Avatar URL support in auth responses

Feature docs: `2026-02-06-design-system-governance.md`

### Implemented API Endpoints (16)

Setup:
- `GET /v1/public/setup-status`
- `POST /v1/setup/bootstrap-token/verify`
- `POST /v1/setup/oidc/configure`
- `POST /v1/setup/owner/start-oidc`
- `POST /v1/setup/owner/verify-oidc`
- `POST /v1/setup/complete`

Auth:
- `GET /v1/auth/oidc/start`
- `POST /v1/auth/oidc/callback`
- `POST /v1/auth/logout`

User Management:
- `GET /v1/users/me`
- `GET /v1/users`
- `POST /v1/users/invite`
- `PATCH /v1/users/{user_id}/role`
- `DELETE /v1/users/{user_id}`
- `POST /v1/users/{user_id}/enable`

Health:
- `GET /healthz`

---

## Phase 2: Trigger Ingestion + Build Planning (`P0`)

Dependency: Phase 1 complete.

- [ ] **2.1 [P0] SCM integration schema** - Add provider/account/installation/repository/webhook tables with encrypted secrets and audit metadata.
- [ ] **2.2 [P0] GitHub integration flow (BYO App)** - Instance-scoped GitHub App onboarding (manifest-assisted or manual app create), installation linking, and permission validation.
- [ ] **2.3 [P0] GitLab integration flow** - Support `gitlab.com` and self-managed base URL with OAuth application or token-based integration plus webhook secret validation.
- [ ] **2.4 [P0] Webhook ingress hardening** - Signature/token verification, idempotency keys, replay window checks, provider event normalization.
- [ ] **2.5 [P0] Build-domain schema** - Add `projects`, `pipelines`, `builds`, `build_jobs`, `runners`, `build_events`, `artifacts` with indexes and FKs.
- [ ] **2.6 [P0] Build state machine contract** - Define strict states/transitions (`queued`, `scheduled`, `assigned`, `running`, `succeeded`, `failed`, `canceled`, `timed_out`, `expired`).
- [ ] **2.7 [P0] Trigger intake endpoints** - Manual/API trigger (`POST /v1/projects/{project_id}/builds`) plus provider webhook trigger endpoint(s).
- [ ] **2.8 [P0] Config snapshot at trigger time** - Resolve and persist immutable workflow/pipeline snapshot (commit SHA + resolved inputs + target platform set).
- [ ] **2.9 [P0] Concurrency and stale-build policy** - Per-branch or per-pipeline cancellation option (`cancel previous` behavior).
- [ ] **2.10 [P0] Build query endpoints** - `GET /v1/builds`, `GET /v1/builds/{build_id}`, `POST /v1/builds/{build_id}/cancel`.

Exit criteria:
- GitHub and GitLab can be connected with secrets stored encrypted and auditable.
- Provider webhooks are received directly by customer backend in both hosted-UI and self-hosted-UI modes.
- Build requests can be created from manual/API/webhook sources.
- Build records are immutable post-creation except status/event updates.
- Cancel and status APIs work against persisted state machine rules.

Feature docs required: SCM Integrations (GitHub/GitLab), Build Lifecycle API, Triggering & Concurrency Policy.

## Phase 3: Scheduler + Runner Execution (`P0`)

Dependency: Phase 2 complete.

- [ ] **3.1 [P0] In-process scheduler/queue** - Tokio channel-based dispatch aligned to ADR-0003.
- [ ] **3.2 [P0] Runner registration/auth** - `POST /v1/runners/register` with scoped runner token issuance/rotation.
- [ ] **3.3 [P0] Heartbeat/capability reporting** - Runner reports host capabilities (macOS version, Xcode, capacity).
- [ ] **3.4 [P0] Claim/lease protocol** - Atomic claim, lease timeout, and safe requeue of abandoned work.
- [ ] **3.5 [P0] Workspace isolation** - Ephemeral per-build working directory and deterministic cleanup.
- [ ] **3.6 [P0] Step executor** - Checkout + script execution with step-level timing and exit code capture.
- [ ] **3.7 [P0] Timeout and cancellation enforcement** - Server-initiated and operator-initiated cancellation.

Exit criteria:
- A registered runner can claim and execute a queued build end-to-end.
- No double-claim on the same job.
- Canceled/timed-out jobs transition to terminal state correctly.

Feature docs required: Runner Protocol, Scheduling & Lease Semantics, Build Isolation.

## Phase 4: Logs + Artifacts + Distribution (`P0`)

Dependency: Phase 3 complete.

- [ ] **4.1 [P0] Structured log ingestion** - Runner log upload endpoint with ordered chunking and truncation safeguards.
- [ ] **4.2 [P0] Live log streaming (SSE)** - `GET /v1/builds/{build_id}/logs/stream`.
- [ ] **4.3 [P0] Artifact capture contract** - Runner finalizes artifact manifest (name, path, checksum, size, type).
- [ ] **4.4 [P0] S3-compatible storage integration** - `aws-sdk-s3` upload/download with signed URLs + TTL.
- [ ] **4.5 [P0] Artifact link APIs** - Authenticated and short-lived public distribution links.
- [ ] **4.6 [P0] Build detail UI** - Status timeline + live logs + artifact list/download actions.

Exit criteria:
- Operator can trigger build, watch live logs, and download produced artifacts.
- Artifact links expire predictably and are auditable.

Feature docs required: Live Build Logs, Artifact Storage & Download Policy.

## Phase 5: Project + Pipeline Product Surface (`P1`)

Dependency: Phases 2-4 complete and stable.

- [ ] **5.1 [P1] Project CRUD APIs** - `GET|POST /v1/projects`, `GET|PATCH|DELETE /v1/projects/{project_id}` with RBAC/audit.
- [ ] **5.2 [P1] Pipeline CRUD APIs** - `GET|POST /v1/projects/{project_id}/pipelines`, plus `GET|PATCH|DELETE` per pipeline.
- [ ] **5.3 [P1] Pipeline schema validation** - Validate triggers, branch/tag patterns, required inputs/defaults.
- [ ] **5.4 [P1] Project/Pipeline UI** - List/detail/create/edit with safe defaults and validation feedback.
- [ ] **5.5 [P1] Trigger settings UI** - Toggle stale-build cancellation and trigger source controls.

Exit criteria:
- Developers can self-serve project/pipeline setup without direct DB/API intervention.
- Invalid pipeline configs are blocked before execution.

Feature docs required: Projects API, Pipelines API, Project/Pipeline UI.

## Phase 6: Operator CLI Completeness (`P1`)

Dependency: Phases 2-5 complete.

- [ ] **6.1 [P1] `oore login`** - OIDC terminal flow.
- [ ] **6.2 [P1] `oore status`** - Instance health, queue depth, runner inventory, recent builds.
- [ ] **6.3 [P1] `oore runner register`** - Register local host as runner.
- [ ] **6.4 [P1] `oore config set/get`** - Stable admin configuration operations.
- [ ] **6.5 [P1] `oore doctor`** - macOS CI diagnostics (Xcode/tooling/signing/connectivity).

Exit criteria:
- Core operator workflows are scriptable from CLI without UI dependency.

Feature docs required: CLI Reference and Runner Setup Guide.

## Phase 7: Reliability + Security + Release Gate (`P2`)

Dependency: Phases 2-6 functional.

- [ ] **7.1 [P2] Retry/rerun support** - Controlled rerun semantics for failed/canceled builds.
- [ ] **7.2 [P2] Manual approval gates (optional V1 scope)** - Pause/resume workflow steps for release-like tasks.
- [ ] **7.3 [P2] E2E test suite** - Playwright + backend integration for full flow.
- [ ] **7.4 [P2] Security hardening** - Input validation audit, path traversal defenses, strict URL TTL enforcement.
- [ ] **7.5 [P2] Operational docs** - Deploy/runbook/recovery guidance.
- [ ] **7.6 [P2] Final release validation** - `make validate` and handoff checklist.

Feature docs required: E2E Tests, Security Hardening, Deployment/Operations.

---

## Gap Summary

| Area | Built | Remaining | Highest Priority |
|------|-------|-----------|------------------|
| Build lifecycle model | Setup/auth state machine only | Build/job/runner/artifact state machines | Phase 2 (`P0`) |
| Triggering | Setup + auth triggers only | Manual/API/webhook build triggers with policy controls | Phase 2 (`P0`) |
| Scheduling/execution | None | Queue, claim/lease, runner execution | Phase 3 (`P0`) |
| Logs/artifacts | None | SSE logs, artifact storage, signed links | Phase 4 (`P0`) |
| Project/pipeline UX | None in mainline | CRUD + validation + trigger settings | Phase 5 (`P1`) |
| CLI operations | Setup + version | login/status/runner/config/doctor | Phase 6 (`P1`) |
| Reliability/security | Partial baseline | E2E, retry, hardening, release gate | Phase 7 (`P2`) |

## Notes

- This roadmap does not change platform-contract decisions; it only corrects implementation order.
- Every user-facing phase must include docs updates under `docs/features/` per policy.
- Each phase is complete only when `make validate` passes.
- ADRs are required only when changing locked `MUST` contract decisions.
- Journey correctness is verified against `docs/v1-user-journey.md` before phase sign-off.
