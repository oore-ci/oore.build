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

### Web App (Bun + Vite)

```bash
bun run dev:web              # dev server for web app (port 3000)
bun run build:web            # production build
cd apps/web && bun run test  # run tests (Vitest)
cd apps/web && bun run lint  # ESLint
cd apps/web && bun run check # Prettier --write + ESLint --fix
```

### Docs Site (VitePress)

```bash
bun run dev:docs             # VitePress dev server (port 4173)
bun run build:docs           # VitePress production build
```

### Backend (Rust + Cargo)

```bash
cargo check --workspace                              # compile check all crates
cargo run -p oored -- run --listen 127.0.0.1:8787    # run daemon
cargo run -p oore -- setup open --ttl 15m            # run operator CLI
```

### Documentation Gate (must pass before finalizing changes)

```bash
bun run docs:check           # validates feature docs against template
```

### Validation Checklist (run before handoff)

```bash
bun run docs:check
bun run build:web
bun run build:docs
cargo check --workspace
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

## Non-Negotiable V1 Rules

- Frontend and backend are strictly separated
- OIDC-only auth (no local passwords)
- macOS-only backend runtime
- TanStack Router file-based routing (no Next.js)
- shadcn uses Base UI primitives (not Radix)
- Every user-facing feature requires a doc in `docs/features/` following `docs/templates/feature-doc-template.md`
- Changing a finalized decision requires: ADR + contract update + feature doc update
