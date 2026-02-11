# V1 Implementation Roadmap

Status: Active - execution-first sequencing for V1 CI completion.
Last assessed: 2026-02-10

## Why This Revision

- Previous sequencing over-weighted project/pipeline CRUD before a complete CI execution loop.
- Industry CI/CD systems (Codemagic, GitHub Actions, Buildkite, CircleCI, GitLab CI) prioritize: trigger -> plan -> queue -> claim -> execute -> logs/artifacts -> completion.
- Code signing work was under-specified even though signing is part of the V1 product contract.
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

### Implemented API Endpoints (47)

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

Integrations:
- `GET /v1/integrations`
- `GET /v1/integrations/{id}`
- `DELETE /v1/integrations/{id}`
- `GET /v1/integrations/{id}/repositories`
- `GET /v1/integrations/{id}/installations`
- `POST /v1/integrations/{id}/installations` (sync)
- `POST /v1/integrations/github/start`
- `POST /v1/integrations/github/complete`
- `GET /v1/integrations/github/create` (browser-navigated, no auth)
- `GET /v1/integrations/github/callback` (browser-navigated, no auth)
- `GET /v1/integrations/github/installed` (browser-navigated, no auth)
- `POST /v1/integrations/gitlab/start`
- `POST /v1/webhooks/github`
- `POST /v1/webhooks/gitlab`

Builds:
- `POST /v1/projects/{project_id}/builds`
- `GET /v1/builds`
- `GET /v1/builds/{build_id}`
- `POST /v1/builds/{build_id}/cancel`

Runners:
- `POST /v1/runners/register`
- `POST /v1/runners/{runner_id}/heartbeat`
- `POST /v1/runners/{runner_id}/claim`
- `POST /v1/runners/{runner_id}/jobs/{job_id}/status`
- `GET /v1/runners/{runner_id}/jobs/{job_id}`
- `GET /v1/runners`

Build Logs:
- `POST /v1/runners/{runner_id}/jobs/{job_id}/logs`
- `GET /v1/builds/{build_id}/logs`
- `GET /v1/builds/{build_id}/logs/stream`
- `POST /v1/builds/{build_id}/stream-token`

Artifacts:
- `POST /v1/runners/{runner_id}/jobs/{job_id}/artifacts`
- `GET /v1/builds/{build_id}/artifacts`
- `POST /v1/artifacts/{artifact_id}/download-link`

Health:
- `GET /healthz`

---

## Phase 2: Trigger Ingestion + Build Planning (`P0`) (Complete)

Dependency: Phase 1 complete.

- [x] **2.1 [P0] SCM integration schema** - Migration 004: integrations, integration_credentials, integration_installations, integration_repositories, integration_webhooks tables with encrypted secrets and audit metadata.
- [x] **2.2 [P0] GitHub integration flow (BYO App)** - GitHub App manifest flow with encrypted credential storage, JWT auth, installation sync, and repo enumeration.
- [x] **2.3 [P0] GitLab integration flow** - Personal token and OAuth modes supporting gitlab.com and self-managed instances with token validation.
- [x] **2.4 [P0] Webhook ingress hardening** - HMAC-SHA256 (GitHub) and token (GitLab) verification, idempotency via UNIQUE constraint, 5-min replay window, NormalizedWebhookEvent, 1MB body limit, async processing.
- [x] **2.5 [P0] Build-domain schema** - Migration 005: projects, pipelines, builds, build_events, runners, artifacts tables with indexes and FKs.
- [x] **2.6 [P0] Build state machine contract** - 9-state machine with validated transitions and optimistic locking in oore-contract.
- [x] **2.7 [P0] Trigger intake endpoints** - `POST /v1/projects/{project_id}/builds` (manual) + webhook-triggered builds via `trigger_build_from_webhook()`.
- [x] **2.8 [P0] Config snapshot at trigger time** - Immutable JSON snapshot with snapshot_version, config_path, trigger metadata, commit SHA, and branch.
- [x] **2.9 [P0] Concurrency and stale-build policy** - `cancel_previous` per-pipeline policy auto-cancels non-terminal builds on same branch.
- [x] **2.10 [P0] Build query endpoints** - `GET /v1/builds` (filtered, paginated), `GET /v1/builds/{build_id}` (with events timeline), `POST /v1/builds/{build_id}/cancel`.

Exit criteria (all met):
- GitHub and GitLab can be connected with secrets stored encrypted and auditable.
- Provider webhooks are received directly by customer backend in both hosted-UI and self-hosted-UI modes.
- Build requests can be created from manual/API/webhook sources.
- Build records are immutable post-creation except status/event updates.
- Cancel and status APIs work against persisted state machine rules.

Feature docs: `2026-02-07-scm-integrations-github-gitlab-v1.md`, `2026-02-07-build-lifecycle-api.md`, `2026-02-07-triggering-and-concurrency-policy.md`.

## Phase 3: Scheduler + Runner Execution (`P0`) (Complete)

Dependency: Phase 2 complete.

- [x] **3.1 [P0] In-process scheduler/event bus** - SQLite-direct job dispatch with broadcast channel for event fan-out (ADR-0003).
- [x] **3.2 [P0] Runner registration/auth** - `POST /v1/runners/register` with scoped runner token issuance/rotation.
- [x] **3.3 [P0] Heartbeat/capability reporting** - Runner reports host capabilities (macOS version, Xcode, capacity). Stale heartbeat detection covers online, busy, and draining runners.
- [x] **3.4 [P0] Claim/lease protocol** - Atomic claim via two-step optimistic locking, lease timeout with runner_id clearing, and safe requeue of abandoned work.
- [x] **3.5 [P0] Workspace isolation** - Ephemeral per-build working directory and deterministic cleanup.
- [x] **3.6 [P0] Step executor** - Commit-pinned checkout (exact SHA when available, branch HEAD fallback) + script execution with step-level timing and exit code capture. Failed builds include accumulated step results.
- [x] **3.7 [P0] Timeout and cancellation enforcement** - Server-initiated and operator-initiated cancellation with mid-step interruption via `tokio::select!`.
- [x] **3.8 [P0] Single-host embedded runner default** - `oored` auto-starts a local embedded runner in default mode so queued builds execute without manual `oore runner start`. Advanced external/hybrid runner modes remain available via `OORED_RUNNER_MODE`.

Exit criteria (all met):
- A registered runner can claim and execute a queued build end-to-end.
- A single-host daemon startup can claim queued builds without a separate manual runner process.
- No double-claim on the same job.
- Canceled/timed-out jobs transition to terminal state correctly.

Feature docs: `2026-02-08-runner-protocol.md`, `2026-02-08-scheduling-and-lease-semantics.md`, `2026-02-08-build-isolation.md`, `2026-02-09-embedded-local-runner-default.md`.

## Phase 4: Logs + Artifacts + Distribution (`P0`) (Complete)

Dependency: Phase 3 complete.

- [x] **4.1 [P0] Structured log ingestion** - Runner log upload endpoint (`POST /v1/runners/{runner_id}/jobs/{job_id}/logs`) with ordered chunking, 10k line cap, 4KB per-line truncation, and INSERT OR IGNORE dedup.
- [x] **4.2 [P0] Live log streaming (SSE)** - `GET /v1/builds/{build_id}/logs/stream` with polling-based SSE, reconnection via Last-Event-ID, keepalive, and auto-close on terminal status. Full log retrieval via `GET /v1/builds/{build_id}/logs`.
- [x] **4.3 [P0] Artifact capture contract** - `POST /v1/runners/{runner_id}/jobs/{job_id}/artifacts` registers artifact manifest (name, type, checksum, size) with validation.
- [x] **4.4 [P0] S3-compatible storage integration** - `aws-sdk-s3` with presigned PUT/GET URLs, configurable endpoint/bucket/region via env vars, graceful fallback when unconfigured.
- [x] **4.5 [P0] Artifact link APIs** - `POST /v1/artifacts/{artifact_id}/download-link` generates 15-min TTL presigned URLs with RBAC check and audit logging. `GET /v1/builds/{build_id}/artifacts` lists artifacts.
- [x] **4.6 [P0] Build detail UI** - Rebuilt build detail page with live SSE log viewer (auto-scroll, stderr highlighting, scroll-lock toggle, polling fallback), artifact table with download actions, duration display, relative timestamps, and auto-refresh for active builds.

Exit criteria (all met):
- Operator can trigger build, watch live logs, and download produced artifacts.
- Artifact links expire predictably and are auditable.
- SSE stream supports reconnection and auto-closes on terminal builds.
- Artifact storage supports local filesystem and S3-compatible backends. If storage is disabled, artifact metadata remains persisted but binary downloads are unavailable.

Feature docs: `2026-02-08-live-build-logs.md`, `2026-02-08-artifact-storage-download-policy.md`.

## Phase 5: Project + Pipeline Product Surface (`P1`) (Complete)

Dependency: Phases 2-4 complete and stable.

- [x] **5.1 [P1] Project CRUD APIs** - `GET|POST /v1/projects`, `GET|PATCH|DELETE /v1/projects/{project_id}` with RBAC/audit.
- [x] **5.2 [P1] Pipeline CRUD APIs** - `GET|POST /v1/projects/{project_id}/pipelines`, plus `GET|PATCH|DELETE` per pipeline.
- [x] **5.3 [P1] Pipeline schema validation** - Validate triggers, branch/tag patterns, required inputs/defaults. Dry-run validation endpoint at `POST /v1/pipelines/validate`.
- [x] **5.4 [P1] Project/Pipeline UI** - List/detail/create/edit with safe defaults and validation feedback.
- [x] **5.5 [P1] Trigger settings UI** - Toggle stale-build cancellation and trigger source controls.
- [x] **5.6 [P1] Runner management UI** - Added `Settings -> Runners` page for runner inventory and external-runner rename flow. Embedded runners are visible but rename-locked.
- [x] **5.7 [P1] Command-center UI redesign** - Reworked main authenticated app pages to table-first, operator-dense layouts with consistent `PageLayout`/`PageHeader` rhythm and settings-page visual alignment.
- [x] **5.8 [P1] File-first pipeline config with UI fallback** - Added strict `.oore.yaml/.oore.yml` resolution, immutable fallback execution snapshot (`snapshot_version=2`), pipeline UI controls for platform toggles/staged commands, and Flutter version control via `.fvmrc` or fallback `flutter_version`.
- [x] **5.9 [P1] Artifact storage settings UI + API** - Added owner/admin artifact storage management (`Settings -> Artifact Storage`) with encrypted credential persistence, runtime backend switching, local filesystem upload/download flow, and S3/R2 support without daemon restart.
- [x] **5.10 [P1] Admin Preferences hub + key storage mode toggle** - Added `Settings -> Preferences` as the admin settings hub, moved artifact storage controls into Preferences, and added owner/admin key storage mode toggle (`keychain` vs `file`) with persisted startup preference and audit logging.

Exit criteria (all met):
- Developers can self-serve project/pipeline setup without direct DB/API intervention.
- Invalid pipeline configs are blocked before execution.

Feature docs: `2026-02-08-projects-api.md`, `2026-02-08-pipelines-api.md`, `2026-02-08-project-pipeline-ui.md`, `2026-02-09-runner-management-ui.md`, `2026-02-09-command-center-ui-redesign.md`, `2026-02-09-file-first-pipeline-config-and-ui-fallback.md`.

## Phase 5.5: Code Signing for Ad-hoc Distribution (`P0`, In Progress)

Dependency: Phases 3-5 complete.

- [x] **5.11 [P0] Android signing bootstrap compatibility fallback (`OORE_ANDROID_*` env contract)** - Runner prepares `android/key.properties` in ephemeral workspace when Flutter Android build commands are present and signing env vars are provided (`OORE_ANDROID_KEYSTORE_PATH` or `OORE_ANDROID_KEYSTORE_BASE64` plus password/alias vars); used when no pipeline-managed signing profile exists.
- [x] **5.12 [P0] Android signing credential management API/UI** - Added encrypted at-rest, pipeline-scoped Android signing profiles (`debug`/`release`) with owner/admin UI workflow in pipeline create/edit dialogs and runner retrieval over authenticated job endpoints.
- [x] **5.13 [P0] iOS signing asset orchestration** - Added pipeline-scoped iOS signing settings (`manual|api|hybrid`), encrypted cert/profile/API-key storage, App Store Connect sync/device registration endpoints, and runner ephemeral keychain/profile materialization.
- [x] **5.14 [P0] iOS ad-hoc export contract** - Runner now enforces signed iOS IPA path (`flutter build ipa` with generated export options), resolves Xcode export method compatibility (`release-testing` preferred, `ad-hoc` fallback), and attaches signing provenance metadata to IPA artifacts.
- [ ] **5.15 [P1] macOS signing + notarization pipeline** - Implement Developer ID signing + `notarytool` submission/polling/stapling with build-surface status.
- [ ] **5.16 [P1] Signing diagnostics (`oore doctor` + runner preflight)** - Add actionable diagnostics for keystore/certificate/profile/notary prerequisites. Progress: runner now emits iOS signing preparation markers and mode/provenance metadata, but dedicated `oore doctor` checks are still pending.

Exit criteria:
- Android release builds can be signed without committing key material to repository history.
- iOS ad-hoc builds produce installable signed IPA artifacts with auditable signing provenance.
- macOS release artifacts are signed and notarized (or fail with explicit diagnostics).

Feature docs: `2026-02-10-android-signing-bootstrap-codemagic-env.md`, `2026-02-10-pipeline-scoped-android-signing-ui.md`, `2026-02-11-ios-ad-hoc-signing-pipeline-api-device-registration.md`.

## Phase 6: Operator CLI Completeness (`P1`)

Dependency: Phases 2-5.5 complete.

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
- [x] **7.7 [P1] Release installer + onboarding bootstrap** - Added tag-driven macOS release packaging (`arm64`, `x86_64`) with checksum assets published to Cloudflare R2 (`dl.oore.build`), release-based `scripts/install.sh` (`curl -fsSL https://oore.build/install | bash`), neutral landing site plan (`apps/site`), and hosted-UI-first onboarding docs.

Feature docs required: E2E Tests, Security Hardening, Deployment/Operations.

## Phase 8: Artifact Distribution Portal (`P2`, Future)

Dependency: Phases 4-7 stable.

Goal: Add a dedicated distribution experience (target URL: `https://artifacts.oore.build` in hosted-UI mode, or customer-hosted equivalent) so QA/viewers can install and track builds without full operator access to the main control UI.

- [ ] **8.1 [P2] Product boundary and tenancy model**
  - Decide portal deployment modes:
    - customer-hosted portal (same instance/domain family)
    - hosted UI portal (`artifacts.oore.build`) that talks to customer backend APIs only
  - Define tenant/instance switching UX for hosted mode.
  - Confirm contract alignment: portal is UI/distribution surface only, not a hosted build runner plane.

- [ ] **8.2 [P2] Release domain model + schema**
  - Add release-oriented entities (draft):
    - `release_channels` (e.g. internal, qa, beta)
    - `release_entries` (maps build -> publish metadata)
    - `release_notes` (manual + generated)
    - `release_assets` (artifact references + checksums + platform metadata)
  - Add migration and indexes for channel listing and newest-per-platform queries.
  - Define retention/archive markers independent from raw build retention.

- [ ] **8.3 [P2] Publish workflow APIs**
  - Add APIs to publish/unpublish a build artifact into a channel.
  - Add APIs to edit release notes/changelog and visibility settings.
  - Add APIs for portal listing:
    - latest release per app/platform/channel
    - release history feed
    - install/download endpoint handshake
  - Keep audit logs for publish/unpublish/note edits/share-link generation.

- [ ] **8.4 [P2] Access model for QA/viewers (simplified)**
  - Add lightweight distribution access path:
    - portal-scoped viewer sessions and/or expiring share links
    - optional email allowlist per project/channel
  - Keep admin/owner controls in main app.
  - Ensure no pipeline edit/build trigger permissions leak into portal roles.

- [ ] **8.5 [P2] iOS/macOS install flow**
  - Implement signed `manifest.plist` generation and hosted install URL flow for iOS ad-hoc installs.
  - Surface required metadata checks before publish (bundle id, version, build number).
  - Add device-facing install CTA compatible with Safari flow.

- [ ] **8.6 [P2] Android install flow**
  - Add APK/AAB distribution UX with checksum/version info.
  - Expose platform-specific install guidance (unknown sources, managed devices).

- [ ] **8.7 [P2] Changelog pipeline**
  - Support manual release notes in UI.
  - Add optional auto-generated changelog from commit range:
    - previous published build SHA -> current SHA
    - grouped by conventional commit type (best effort)
  - Provide markdown renderer with sanitized output.

- [ ] **8.8 [P2] Storage and delivery strategy**
  - Support delivery backends already present in instance settings (`local`, `s3`, `r2`).
  - Add optional public-read mode per channel/project with explicit warning.
  - Add signed-download mode with TTL and revoke support.
  - Ensure dedupe/checksum behavior remains consistent for release assets.

- [ ] **8.9 [P2] Portal frontend**
  - New portal shell optimized for non-operator users:
    - app list
    - channel picker
    - latest build/install CTA
    - release history + changelog
  - Mobile-first QA layout (Safari/iPhone first-class).
  - Keep design tokens/theme aligned with main app but with simplified navigation.

- [ ] **8.10 [P2] Observability + abuse controls**
  - Metrics:
    - release publish count
    - install/download attempts
    - success/failure by platform
  - Controls:
    - download rate limit
    - link expiry + one-click revoke
    - suspicious access logging

- [ ] **8.11 [P2] Docs and onboarding**
  - Add docs for:
    - publish workflow
    - channel strategy
    - iOS ad-hoc prerequisites
    - QA viewer onboarding
  - Add operator checklist for safe public/internal distribution settings.

Exit criteria:
- QA/viewers can discover and install latest builds from a dedicated portal without full control-plane access.
- Owner/admin can publish/unpublish releases with auditable history.
- iOS Safari install flow works for ad-hoc distribution where signing/profile prerequisites are met.
- Changelog is visible per published release (manual or generated).
- Storage/security policy (public vs signed links) is explicit per channel and enforced.

---

## Gap Summary

| Area | Built | Remaining | Highest Priority |
|------|-------|-----------|------------------|
| Build lifecycle model | 9-state machine with transitions, optimistic locking, audit trail | None for V1 | Complete |
| Triggering | Manual/API + webhook (GitHub/GitLab) with concurrency policy | Schedule triggers | Phase 5+ |
| SCM integration | GitHub App + GitLab (token/OAuth) with encrypted secrets | None for V1 | Complete |
| Scheduling/execution | In-process scheduler, runner registration, claim/lease, workspace isolation, step executor, timeout enforcement | None for V1 | Complete |
| Logs/artifacts | SSE streaming, log ingestion, local or S3-compatible artifacts, signed URLs, live UI | None for V1 | Complete |
| Project/pipeline UX | CRUD APIs, validation, trigger settings, management UI | Complete | Complete |
| Code signing | Android bootstrap + pipeline-scoped encrypted signing profiles (debug/release) with UI management | iOS signing/profiles, macOS notarization | Phase 5.5 (`P0/P1`) |
| Distribution portal | Build details + artifact downloads exist in main app | Dedicated QA/viewer release portal, publish workflow, changelogs, install-first UX | Phase 8 (`P2`, Future) |
| CLI operations | Setup + runner register/start | login/status/config/doctor | Phase 6 (`P1`) |
| Installer/onboarding | Release-based macOS installer, interactive first-run prompts, hosted-UI onboarding docs | Signed installer provenance and upgrade channels | Phase 7 (`P1/P2`) |
| Reliability/security | Partial baseline + release packaging CI | E2E, retry, hardening, release gate | Phase 7 (`P2`) |

## Notes

- This roadmap does not change platform-contract decisions; it only corrects implementation order.
- Every user-facing phase must include docs updates under `docs/features/` per policy.
- Each phase is complete only when `make validate` passes.
- ADRs are required only when changing locked `MUST` contract decisions.
- Journey correctness is verified against `docs/v1-user-journey.md` before phase sign-off.
