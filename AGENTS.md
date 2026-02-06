# AGENTS.md

This file guides future coding sessions for `oore.build`.

## Read First (Mandatory)

Before making any code or architecture change, read:

- `docs/platform-contract.md`
- `docs/strict-guidelines.md`
- `docs/documentation-policy.md`

Treat those as the source of truth.

## Non-Negotiable Rules

- Keep frontend and backend cleanly separated.
- V1 auth is OIDC-only.
- V1 backend runtime target is macOS.
- Hosted offering at `ci.oore.build` is UI-only.
- Keep command surfaces stable:
- `oored` for daemon/runtime lifecycle.
- `oore` for operator/setup/admin flows.

## Frontend Rules (V1)

- Use TanStack Router file-based routing.
- Do not introduce Next.js for V1.
- Use Bun as package manager/runtime for frontend toolchain.
- Use TanStack Query for server state and Zustand for UI-local state.
- Use shadcn with Base UI primitives (not Radix).
- Keep `apps/web` and `apps/docs-site` aligned on shared shadcn constraints:
- `style: base-vega`
- `iconLibrary: hugeicons`
- `theme: amber`
- `baseColor: neutral`
- `menuAccent: subtle`
- `menuColor: default`
- `radius: none`
- `font: inter`
- Docs framework is VitePress under `apps/docs-site/docs`.

## Frontend Design System (Mandatory)

- Read `DESIGN.md` before any frontend UI work.
- Follow the shadcn-first component selection rule: check registry -> install -> use.
- Never create custom dialogs, dropdowns, drawers, or tables when shadcn has equivalents.
- Use Hugeicons for all icons. No inline SVG icons.
- Use shadcn Form component with react-hook-form + zod for all forms.
- Use Skeleton/Spinner for loading states, Toast for transient feedback, Alert for persistent feedback.
- All colors must use the token system from `styles.css`. No hard-coded Tailwind color classes.
- Support dark mode using token-based styling only.

## Documentation and Governance Rules

- Every user-facing feature MUST add/update a doc in `docs/features/`.
- Feature docs MUST follow `docs/templates/feature-doc-template.md`.
- If code changes platform decisions or strict rules:
- update `docs/platform-contract.md`
- add/update a feature doc
- add an ADR if changing a `MUST`-level rule
- Run docs gate locally before finalizing:
- `bun run docs:check`

## Backend Bootstrap Direction

- Rust workspace crates:
- `crates/oored`
- `crates/oore`
- `crates/oore-contract`
- Keep `/v1/public/setup-status` non-sensitive.
- Setup mutating endpoints must be token-gated and disabled after `ready`.

## Makefile Maintenance

- All build, test, lint, and dev commands must have a corresponding `make` target in the root `Makefile`.
- When adding new scripts or tooling, update the Makefile.
- `make validate` is the single command for the full pre-handoff checklist.

## Validation Checklist (Before Handoff)

- Run `make validate` (runs docs-check, build-web, build-docs, cargo-check).

## V1 Roadmap

- Implementation roadmap is tracked in `docs/v1-roadmap.md`.
- Check off completed items and update gap summary after each phase.
- When adding new work items, add them to the appropriate phase or create a new phase.
- Roadmap does NOT override `docs/platform-contract.md` — it sequences existing commitments.
