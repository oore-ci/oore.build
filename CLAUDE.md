# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Mandatory Pre-Reading

Before making any code or architecture change, read these files (they are the source of truth):

- `docs/README.md` — internal docs pointers (Linear-first) + change ledger
- Docs Index (Linear): https://linear.app/oorebuild/document/docs-index-linear-first-457d9edc9cda
- `AGENTS.md` — agent-specific guidance
- V1 Roadmap (Linear): https://linear.app/oorebuild/document/v1-implementation-roadmap-5e4fa12cdb04

## What This Project Is

Oore CI is a self-hosted, Flutter-first mobile CI and internal app distribution platform. V1 targets Android, iOS, and macOS builds. The backend runs on macOS only in V1. The hosted offering at `ci.oore.build` is UI-only (frontend connects to customer's self-hosted backend).

## Build / Test / Lint Commands

All common commands have `make` targets. Use `make <target>` from the repo root.

## Release Channels (Alpha/Beta/Stable)

Release automation is GitHub Actions-driven and tag-based. Branch merges cut tags automatically, which triggers the release pipeline and publishes GitHub Releases.

- `alpha` -> `vX.Y.Z-alpha.N` prerelease tags (and prerelease GitHub Releases)
- `beta` -> `vX.Y.Z-beta.N` prerelease tags (and prerelease GitHub Releases)
- `stable` -> `vX.Y.Z` production tags (and production GitHub Releases)
- `master` is a playground branch: validated by CI but not auto-tagged.

Source of truth doc (Linear-first):
https://linear.app/oorebuild/document/release-channels-alpha-beta-stable-via-woodpecker-github-releases-993db297927a

| Target | What it does |
|---|---|
| `make dev-web` | Web app dev server (web.oore.localhost via portless) |
| `make dev-docs` | VitePress dev server (docs.oore.localhost via portless) |
| `make build-web` | Production build (web) |
| `make build-docs` | VitePress production build |
| `make test-web` | Run web app tests (Vitest) |
| `make lint-web` | ESLint |
| `make fix-web` | Prettier --write + ESLint --fix |
| `make cargo-check` | Compile check all Rust crates |
| `make run-daemon` | Run oored on 127.0.0.1:8787 |
| `make run-cli` | Run oore setup token --ttl 15m |
| `make docs-check` | Validate internal docs pointers + change ledger |
| `make ui-init` | Re-initialize shadcn from shared preset |
| `make gen-openapi` | Regenerate OpenAPI spec into docs site |
| `make build` | build-web + build-docs + cargo-check |
| `make check` | lint-web + cargo-check |
| `make validate` | Full pre-handoff validation (docs-check + builds + cargo-check) |
| `make portless-proxy` | Start the portless reverse proxy daemon |
| `make portless-alias-api` | Alias oored daemon as api.oore.localhost:1355 |
| `make portless-list` | Show active portless routes |

### Validation Checklist (run before handoff)

```bash
make validate
```

## Architecture

### Workspace Layout

- **`apps/web`** — primary product web UI (React 19 + TanStack Router + Vite)
- **`apps/docs-site`** — static documentation site (VitePress under `apps/docs-site/docs/`)
- **`crates/oored`** — daemon runtime and control plane (Axum HTTP server)
- **`crates/oore`** — operator CLI/TUI for setup and admin (Clap)
- **`crates/oore-contract`** — shared backend data contracts (pure Serde structs)
- **`docs/`** — repo pointer index + `docs/changes.md` change ledger (internal docs live in Linear)
- **`tools/`** — build and validation scripts

### Frontend Stack

- **Routing:** TanStack Router (file-based, NOT Next.js — this is a hard rule)
- **Server state:** TanStack Query
- **UI-local state:** Zustand (never use for server data)
- **Components:** shadcn with Base UI primitives (NOT Radix — Radix is removed)
- **Styling:** Tailwind CSS v4
- **Icons:** Hugeicons
- **Package manager:** Bun
- **Multi-instance support:** frontend must isolate auth/session and query cache per backend instance

### shadcn Constraints (apps/web — shared preset applies)

`style: base-vega`, `iconLibrary: hugeicons`, `theme: amber`, `baseColor: neutral`, `menuAccent: subtle`, `menuColor: default`, `radius: default`, `font: inter`

Vega is the generator and runtime default. The product supports the exact shadcn Create runtime component styles through neutral `cn-*` hooks and one managed `style-*` body class. Amber is the default browser-local color theme.

Run `bun run ui:init` to re-initialize shadcn from the shared preset.

### Frontend Design System

`DESIGN.md` is the mandatory design governance document for `apps/web`. Read it before making any UI change. It specifies: shadcn-first component selection, theming (oklch CSS variables, dark mode), typography/spacing/layout conventions, form/loading/error/feedback patterns, icon usage (Hugeicons only), and anti-patterns to avoid.

### Backend Stack

- **Language:** Rust (edition 2024)
- **Async runtime:** Tokio
- **Web framework:** Axum 0.8
- **CLI:** Clap 4.5
- **Auth:** OIDC for any non-loopback access; loopback-only local login exists (auto-bootstrap requires Local Only mode; no passwords)
- **Command surfaces are stable contracts:**
  - `oored` — daemon/runtime lifecycle
  - `oore` — operator/setup/admin flows

### Setup State Machine

`uninitialized` → `bootstrap_pending` → `idp_configured` → `owner_created` → `ready`

Setup mutating endpoints are token-gated and auto-disabled after `ready` (exception: Local Only mode may auto-complete setup on first loopback local login). The `/v1/public/setup-status` endpoint is always public and non-sensitive.

## Makefile Maintenance

When adding new build scripts, test commands, or tooling workflows, add a corresponding `make` target to the root `Makefile`. Keep `make validate` as the single command for the full pre-handoff checklist.

## Non-Negotiable V1 Rules

- Frontend and backend are strictly separated
- OIDC for any non-loopback access; loopback-only local login exists (auto-bootstrap requires Local Only mode; no local passwords)
- macOS-only backend runtime
- TanStack Router file-based routing (no Next.js)
- shadcn uses Base UI primitives (not Radix)
- Every user-facing feature requires a Linear feature doc (template):
  - https://linear.app/oorebuild/document/feature-doc-template-9f1845da4b46
- Any code change under `apps/`, `crates/`, `tools/`, etc. MUST update `docs/changes.md`
- Changing a finalized decision requires: ADR + contract update + feature doc update (all in Linear)
- **Any endpoint change (create/update/remove) in `crates/oored` must include an update to the OpenAPI spec** — update `crates/oored/src/bin/openapi_export.rs`, run `make gen-openapi`, and commit the regenerated `apps/docs-site/docs/public/openapi.json`

## V1 Roadmap

V1 roadmap lives in Linear. Update it when completing phases or discovering new work:

- https://linear.app/oorebuild/document/v1-implementation-roadmap-5e4fa12cdb04

The roadmap sequences existing platform-contract commitments — it does not override them.

## Documentation Standards
When generating or updating documentation, always check for existing CLAUDE.md, README.md, and docs/ directory first. Preserve existing content and append/update rather than overwriting.

## Project Structure
This project primarily uses Markdown and JSON files. When creating new files, follow existing naming conventions and directory structure. Always validate JSON files with `cat <file> | python3 -m json.tool` before committing.

After generating documentation, always run a final review pass: check for broken links, consistent formatting, accurate code references, and completeness against the source code.
