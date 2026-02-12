# oore.build Platform Contract

Status: Active contract with locked V1 decisions.
Last updated: 2026-02-06

## 0) Contract Discipline

- `MUST` means non-optional.
- `SHOULD` means strongly recommended unless there is a justified exception.
- Any change to a `MUST` rule requires:
- an ADR entry
- a contract update in this file
- a feature doc update in `docs/features/`

## 1) Product Definition

oore.build is a self-hosted, Flutter-first mobile CI platform focused on internal distribution of ad-hoc apps, especially for iOS/macOS teams that need control over infrastructure and security.

## 2) V1 Goals

- Build and test Flutter projects for `Android`, `iOS`, and `macOS`.
- Support signing and artifact publishing.
- Provide simple internal distribution through install/download links.
- Provide clear role-based access control.
- Keep architecture extensible for future project types.

## 3) V1 Non-Goals

- First-class React Native/native iOS/native Android/Tauri build pipelines.
- Full enterprise release orchestration (promotions, environments, release trains).
- MDM-style device management workflows.

## 4) Deployment and Product Model

- Self-hosted is the primary product.
- Hosted offering at `ci.oore.build` is UI-only.
- Hosted offering does not run customer builds or runners.
- Customer backend runs on customer-owned macOS hosts in V1.

## 5) Runtime Components

- `oored`: daemon and control-plane runtime.
- `oore`: operator CLI/TUI for setup and administration.
- Hosted web UI: remote frontend that connects to a customer backend.

## 6) Command Contract

### `oored` (daemon)

- `oored run`
- `oored install-service`
- `oored uninstall-service`
- `oored version`

### `oore` (operator client)

- `oore setup token --ttl <duration>`
- `oore setup` (interactive local fallback)
- `oore login`
- `oore status`
- `oore runner register`
- `oore config set`
- `oore config get`
- `oore doctor`

Command stability rules:

- `oored` and `oore` command names are stable contract surfaces.
- New commands may be added, but existing command names and core semantics must remain backward compatible in V1.

## 7) Auth and Bootstrap Contract

### Auth policy (locked)

- V1 is `OIDC-only`.
- No regular local username/password auth in V1.

### Bootstrap policy (locked)

- Setup is enabled through a temporary setup window.
- Setup token is one-time and time-bound (TTL).
- Setup endpoints are disabled automatically after instance reaches `ready`.

### Recovery policy (locked)

- Break-glass is opt-in runtime activation.
- Recovery is not always-on.
- Recovery requires local host/operator access and short-lived activation.

## 8) Roles and Permissions (V1)

- `owner`: complete instance control and security settings.
- `admin`: full org/project control except owner-only actions.
- `developer`: manage repos/pipelines and trigger builds.
- `qa_viewer`: read builds and download/install allowed artifacts.

## 9) Setup State Model

Read-only public setup status may expose only non-sensitive progress:

- `uninitialized`
- `bootstrap_pending`
- `idp_configured`
- `owner_created`
- `ready`

No secrets or credentials are returned from public status endpoints.

## 10) Backend Technology Contract (locked direction)

Language and platform:

- Rust all-in for backend, daemon, and CLI.
- V1 backend runtime supported on macOS only.

Core backend stack:

- Runtime: `tokio`
- API: `axum`
- Database: `SQLite` with `sqlx` (see [ADR-0001](../adr/0001-sqlite-over-postgresql-for-v1.md))
- Queue/event bus: in-process (`tokio` channels) (see [ADR-0003](../adr/0003-in-process-queuing-over-nats-for-v1.md))
- Artifact storage: S3-compatible using `aws-sdk-s3`
- Auth: OIDC via `openidconnect`
- RBAC policy layer: `casbin-rs`
- Observability: `tracing`, OpenTelemetry, Prometheus metrics

Backend implementation rules:

- Backend runtime support in V1 is macOS only.
- Public setup read endpoint must remain non-sensitive.
- Setup mutating endpoints must be disabled once state is `ready`.
- Hosted UI flow and local CLI setup flow must both be supported.

## 11) Frontend Technology Contract (locked direction)

- Framework: React + TypeScript
- Build tool: Vite
- Routing: TanStack Router (explicitly no Next.js)
- Server state and data fetching: TanStack Query
- Client/UI state: Zustand
- UI system: shadcn/ui with Base UI primitives
- Validation/forms: Zod + React Hook Form
- Package manager/runtime for frontend toolchain: Bun
- Unit/integration tests: `bun test`
- E2E tests: Playwright

## 11.1) Frontend Implementation Rules (strict)

- Router mode MUST be file-based TanStack Router.
- Next.js is out of scope for V1 frontend architecture.
- Shared shadcn configuration MUST be equivalent across `apps/web` and `apps/docs-site`.
- shadcn primitive base MUST be Base UI (not Radix).
- Shared style constraints MUST include:
- `style: base-vega`
- `iconLibrary: hugeicons`
- `baseColor: neutral`
- `theme: amber`
- `menuAccent: subtle`
- `menuColor: default`
- `radius: none`
- `font: inter`

## 12) Frontend State Boundaries

- TanStack Query handles API state, caching, and invalidation.
- Zustand handles UI-local state only.
- Example UI-local state: active wizard step.
- Example UI-local state: local panel/layout preferences.
- Example UI-local state: selected instance context.

Server data must not be duplicated in Zustand.

## 12.1) Multi-Instance Frontend Requirement (V1)

- Frontend must support connecting to multiple backend instances.
- Frontend must allow add, remove, and switch active instance.
- Auth/session tokens are isolated per instance.
- Query caches are partitioned by instance identifier.
- Every API request is scoped to the currently active instance context.
- UI must show current active instance clearly in navigation/header.

## 13) API Boundary Contract

- Frontend and backend are cleanly separated.
- Hosted UI communicates with customer backend over HTTPS APIs.
- Public endpoint(s) may expose setup progress only.
- Mutating setup endpoints require setup token and are disabled after `ready`.
- Live build output should start with SSE for simplicity and operability.

## 14) Security Principles (V1)

- Least privilege by role and operation.
- Explicit audit logging for privileged actions.
- Encrypt secrets at rest.
- Short-lived tokens wherever possible.
- One-time bootstrap tokens with TTL.
- No sensitive configuration on public status endpoints.
- CORS and origin policy restricted to approved frontend origins.

## 15) Extensibility Direction

Architecture must remain open for:

- Additional project types beyond Flutter.
- Additional backend host platforms after V1.
- Desktop client (Tauri) after web-first release.

## 15.1) Documentation Contract and Release Gate (V1)

- Documentation is a required deliverable for every user-facing feature.
- A static documentation website is part of the repository and release process.
- Documentation website framework for V1 is `VitePress` (`apps/docs-site/docs`).
- Every feature must include a document in the predefined feature format.
- CI must fail when code changes are present without required feature documentation updates.
- CI must fail when feature documentation does not match required template sections.

Additional documentation rules:

- This contract file is the source of truth for finalized platform decisions.
- `docs/documentation-policy.md` defines the minimum release gate for docs.
- Feature docs describe increments; this contract describes cross-cutting invariants.

## 16) Finalized V1 Decisions

### Runner protocol

- V1 uses `HTTPS JSON` with pull-based runner scheduling.
- Runners poll/claim jobs from backend and push logs/status/artifact metadata back to backend APIs.
- Single-host default operation may use an embedded local runner started by `oored`; external runners remain supported for explicit multi-runner topologies.
- `gRPC` is deferred; transport abstraction should be kept clean to allow a future switch.

### Build execution isolation on macOS

- V1 uses process isolation, not VM/container isolation.
- Jobs run as a dedicated non-root runner account.
- Each job gets an isolated workspace directory and ephemeral signing/keychain material.
- Workspace and temporary signing assets are destroyed after job completion.
- Trust model for V1 is single-tenant per backend instance.

### Artifact access model

- V1 uses short-lived signed URLs after RBAC authorization.
- Backend authorizes request, then issues time-bound signed download/upload URLs.
- Public build visibility, when enabled, is implemented with explicit project/build policy and short-lived links.

### Tenant model

- V1 is `single organization per backend instance`.
- Multi-org/workspace tenancy is deferred, but internal IDs should avoid hard-coding assumptions that block future tenancy.

### API contract and endpoint baseline

- API is versioned under `/v1`.
- Setup/public:
- `GET /v1/public/setup-status`
- Setup/mutating (setup mode only, token required):
- `POST /v1/setup/bootstrap-token/verify`
- `POST /v1/setup/oidc/configure`
- `POST /v1/setup/owner/start-oidc`
- `POST /v1/setup/owner/verify-oidc`
- `POST /v1/setup/complete`
- Auth:
- `GET /v1/auth/oidc/start`
- `GET /v1/auth/oidc/callback`
- `POST /v1/auth/logout`
- Projects/pipelines/builds:
- `GET|POST /v1/projects`
- `GET|PATCH|DELETE /v1/projects/{project_id}`
- `GET|POST /v1/projects/{project_id}/pipelines`
- `POST /v1/projects/{project_id}/builds`
- `GET /v1/builds`
- `GET /v1/builds/{build_id}`
- `POST /v1/builds/{build_id}/cancel`
- Logs/artifacts/runners:
- `GET /v1/builds/{build_id}/logs/stream` (SSE)
- `GET /v1/builds/{build_id}/artifacts`
- `POST /v1/artifacts/{artifact_id}/download-link`
- `POST /v1/runners/register`
- `POST /v1/runners/{runner_id}/heartbeat`
- `POST /v1/runners/{runner_id}/claim`
- `POST /v1/runners/{runner_id}/jobs/{job_id}/status`
- `POST /v1/runners/{runner_id}/jobs/{job_id}/logs`

## 17) Remaining Open Items

- None for the V1 architecture contract.
- Future updates should be tracked as explicit ADRs or contract revisions.
