# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Mandatory Pre-Reading

Before making any code or architecture change, read these files (they are the source of truth):

- `docs/platform-contract.md` — finalized V1 decisions
- `docs/strict-guidelines.md` — mandatory rules
- `docs/documentation-policy.md` — CI documentation requirements
- `AGENTS.md` — agent-specific guidance

## What This Project Is

oore.build is a self-hosted, Flutter-first mobile CI and internal app distribution platform. V1 targets Android, iOS, and macOS builds. The backend runs on macOS only in V1. The hosted offering at `ci.oore.build` is UI-only (frontend connects to customer's self-hosted backend).

## Build / Test / Lint Commands

All common commands have `make` targets. Use `make <target>` from the repo root.

| Target | What it does |
|---|---|
| `make dev-web` | Web app dev server (port 3000) |
| `make dev-docs` | VitePress dev server (port 4173) |
| `make build-web` | Production build (web) |
| `make build-docs` | VitePress production build |
| `make test-web` | Run web app tests (Vitest) |
| `make lint-web` | ESLint |
| `make fix-web` | Prettier --write + ESLint --fix |
| `make cargo-check` | Compile check all Rust crates |
| `make run-daemon` | Run oored on 127.0.0.1:8787 |
| `make run-cli` | Run oore setup open --ttl 15m |
| `make docs-check` | Validate feature docs against template |
| `make ui-init` | Re-initialize shadcn from shared preset |
| `make build` | build-web + build-docs + cargo-check |
| `make check` | lint-web + cargo-check |
| `make validate` | Full pre-handoff validation (docs-check + builds + cargo-check) |

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
- **`docs/features/`** — required feature documentation entries
- **`scripts/`** — build and validation scripts

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

`style: base-vega`, `iconLibrary: hugeicons`, `theme: amber`, `baseColor: neutral`, `menuAccent: subtle`, `menuColor: default`, `radius: none`, `font: inter`

Run `bun run ui:init` to re-initialize shadcn from the shared preset.

### Backend Stack

- **Language:** Rust (edition 2024)
- **Async runtime:** Tokio
- **Web framework:** Axum 0.8
- **CLI:** Clap 4.5
- **Auth:** OIDC-only (no local username/password in V1)
- **Command surfaces are stable contracts:**
  - `oored` — daemon/runtime lifecycle
  - `oore` — operator/setup/admin flows

### Setup State Machine

`uninitialized` → `bootstrap_pending` → `idp_configured` → `owner_created` → `ready`

Setup mutating endpoints are token-gated and auto-disabled after `ready`. The `/v1/public/setup-status` endpoint is always public and non-sensitive.

## Makefile Maintenance

When adding new build scripts, test commands, or tooling workflows, add a corresponding `make` target to the root `Makefile`. Keep `make validate` as the single command for the full pre-handoff checklist.

## Non-Negotiable V1 Rules

- Frontend and backend are strictly separated
- OIDC-only auth (no local passwords)
- macOS-only backend runtime
- TanStack Router file-based routing (no Next.js)
- shadcn uses Base UI primitives (not Radix)
- Every user-facing feature requires a doc in `docs/features/` following `docs/templates/feature-doc-template.md`
- Changing a finalized decision requires: ADR + contract update + feature doc update
