# V1 Implementation Roadmap

Status: Active — tracks remaining work to V1 completion.
Last assessed: 2026-02-06

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

## Phase 1: Data Model + RBAC (Complete)

- [x] **1.1 Expand SQLite schema** — Added `users`, `sessions`, `audit_logs` tables via `002_users_sessions_audit.sql`
- [x] **1.2 Wire RBAC middleware** — Casbin-rs policy engine integrated into Axum; `AuthUser` extractor validates session + role on every request; per-route guards for owner/admin/developer/qa_viewer
- [x] **1.3 User management endpoints** — Invite, list, get profile, update role, disable (soft-delete), re-enable; all with RBAC checks
- [x] **1.4 Persistent sessions** — SQLite-backed sessions with user FK, CASCADE delete, status check on validate; survives daemon restarts
- [x] **1.5 Auth store per instance** — Frontend auth tokens isolated per instance in localStorage; auth store syncs on instance switch
- [x] **1.6 Login + callback flow** — `/login` page, OIDC callback (POST), invited user auto-activation on first login, unknown identity rejection
- [x] **1.7 User management UI** — `/settings/users` page with invite form, role dropdown, disable/enable buttons, confirmation dialogs for destructive actions, inline feedback alerts
- [x] **1.8 Header auth UI** — Current user email + sign-out button, role-based "Users" nav link
- [x] **1.9 Audit logging** — Security-relevant actions logged: invite, role change, disable, enable, activation, owner creation
- [x] **1.10 VitePress docs** — Users API, RBAC, User Management feature pages; updated OIDC, Auth API, Security, and API overview pages

Feature docs: `2026-02-06-rbac-and-user-management.md`, `2026-02-06-session-persistence.md`

### Implemented API Endpoints (16/25+)

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

## Phase 2: Project + Pipeline CRUD

Dependency: Phase 1 (schema + RBAC) ✅

- [ ] **2.1 Project endpoints** — `GET|POST /v1/projects`, `GET|PATCH|DELETE /v1/projects/{project_id}` with RBAC
- [ ] **2.2 Pipeline endpoints** — `GET|POST /v1/projects/{project_id}/pipelines` with build config validation
- [ ] **2.3 Project list + detail UI** — Dashboard page, project cards, create/edit forms
- [ ] **2.4 Pipeline editor UI** — YAML editor for pipeline definitions (keep simple for V1)

Feature docs required: Projects API, Pipelines API, Project UI

## Phase 3: Build Execution Engine

Dependency: Phase 2 (projects + pipelines exist to trigger builds from)

- [ ] **3.1 In-process job queue** — Tokio channel-based dispatch per ADR-0003; job state machine (pending → claimed → running → succeeded/failed)
- [ ] **3.2 Build trigger endpoint** — `POST /v1/projects/{project_id}/builds` creates build record + enqueues job
- [ ] **3.3 Build list + detail endpoints** — `GET /v1/builds`, `GET /v1/builds/{build_id}`, `POST /v1/builds/{build_id}/cancel`
- [ ] **3.4 Build detail UI** — Status, duration, metadata, cancel button

Feature docs required: Job Queue, Build Lifecycle

## Phase 4: Runner Protocol

Dependency: Phase 3 (job queue exists for runners to pull from)

- [ ] **4.1 Runner registration** — `POST /v1/runners/register` with auth token issuance
- [ ] **4.2 Runner heartbeat** — `POST /v1/runners/{runner_id}/heartbeat` with capacity reporting
- [ ] **4.3 Pull-based job claiming** — `POST /v1/runners/{runner_id}/claim` returns next pending job
- [ ] **4.4 Job status reporting** — `POST /v1/runners/{runner_id}/jobs/{job_id}/status` updates build state
- [ ] **4.5 Build workspace management** — Ephemeral per-job directories, cleanup on completion
- [ ] **4.6 Runner management UI** — List runners, health status, job history

Feature docs required: Runner Registration, Job Scheduling, Build Isolation

## Phase 5: Logs + Artifacts

Dependency: Phase 4 (runners produce logs and artifacts)

- [ ] **5.1 Log upload from runner** — `POST /v1/runners/{runner_id}/jobs/{job_id}/logs`
- [ ] **5.2 Live log streaming** — `GET /v1/builds/{build_id}/logs/stream` via SSE
- [ ] **5.3 Log viewer UI** — Real-time build output in build detail page
- [ ] **5.4 Artifact upload** — Runner pushes artifact metadata after build
- [ ] **5.5 Artifact storage (S3)** — `aws-sdk-s3` integration with signed upload/download URLs
- [ ] **5.6 Artifact browser UI** — List artifacts, download links per build
- [ ] **5.7 Artifact download endpoint** — `POST /v1/artifacts/{artifact_id}/download-link`

Feature docs required: Live Build Logs, Artifact Storage

## Phase 6: CLI Completeness

Dependency: Phases 1-5 (endpoints exist for CLI to call)

- [ ] **6.1 `oore login`** — OIDC flow from terminal (browser redirect + callback)
- [ ] **6.2 `oore status`** — Instance health, runner count, recent builds
- [ ] **6.3 `oore runner register`** — Register current host as runner
- [ ] **6.4 `oore config set/get`** — Read/write instance configuration
- [ ] **6.5 `oore doctor`** — System diagnostics (Xcode, signing, connectivity)

Feature docs required: CLI Completeness

## Phase 7: Polish + Release Readiness

Dependency: Phases 1-6 functional

- [ ] **7.1 E2E test suite** — Playwright tests for setup flow, build trigger, log streaming, artifact download
- [ ] **7.2 Security hardening** — Input validation audit, path traversal checks, signed URL TTL enforcement
- [ ] **7.3 Operator documentation** — Deployment guide, runner setup guide, OIDC provider config guide
- [ ] **7.4 Final `make validate`** — All docs, builds, and checks green

Feature docs required: E2E Tests, Security Hardening, Deployment Guide

---

## Gap Summary

| Area | Built | Remaining | Blocked By |
|------|-------|-----------|------------|
| API endpoints | 16 | ~15 (projects, pipelines, builds, runners, artifacts) | — |
| SQLite tables | setup + users + sessions + audit_logs | ~5 (projects, pipelines, builds, runners, artifacts) | — |
| RBAC | Casbin enforced on all user endpoints | Extend to project/build/runner endpoints | Phase 2+ |
| Frontend pages | setup + dashboard + login + callback + settings/users | ~5 (projects, pipelines, builds, runners, artifacts) | API endpoints |
| CLI commands | setup + version | 5 commands | API endpoints |
| Tests | unit (API, stores, auth store) | E2E suite | features to test |
| VitePress docs | Setup, OIDC, Multi-Instance, RBAC, User Mgmt, 3 API refs | Project/Build/Runner docs | features to document |

## Notes

- Each phase produces feature docs per `docs/documentation-policy.md`
- Each phase ends with `make validate` passing
- ADRs required only when changing locked contract decisions
- This roadmap does NOT change any platform-contract decisions; it sequences existing commitments
