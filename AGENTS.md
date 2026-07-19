# AGENTS.md

This file guides future coding sessions for `oore.build`.

## Read First (Mandatory)

Before making any code or architecture change, read the private docs index with
the Obsidian CLI:

```sh
obsidian vault="oore.build" read file="Oore Docs Index"
```

Treat that index and its linked notes as the source of truth.

## Non-Negotiable Rules

- Keep frontend and backend cleanly separated.
- V1 auth is OIDC for any non-loopback access (Remote mode). Loopback-only local login is supported; when setup is incomplete it is only available in Local Only mode (no passwords).
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
- Keep `apps/web` aligned with its checked-in shadcn registry configuration. Vega is the generator and runtime default; installed primitives must retain the neutral `cn-*` hooks required by the runtime style registry.
- Supported browser-local component styles mirror shadcn Create exactly: Vega, Nova, Maia, Lyra, Mira, Luma, Sera, and Rhea.
- `iconLibrary: lucide`
- `baseColor: neutral`
- `menuAccent: subtle`
- `menuColor: default`
- Use shadcn Create's Neutral base plus its exact Theme picker values for browser-local color themes. Amber is Oore's default; component geometry follows the selected Create style, while Inter for UI text and JetBrains Mono for machine data remain product choices.
- Docs framework is VitePress under `apps/docs-site/docs`.
- The public site is a static Vite application under `apps/site`; neither the docs site nor the public site should carry the React/shadcn application scaffold.

## Frontend Design System (Mandatory)

- Read `DESIGN.md` before any frontend UI work.
- Follow the shadcn-first component selection rule: check registry -> install -> use.
- Never create custom dialogs, dropdowns, drawers, or tables when shadcn has equivalents.
- Use Lucide for all icons. No inline SVG icons.
- Use shadcn Form component with react-hook-form + zod for all forms.
- Use Skeleton/Spinner for loading states, Toast for transient feedback, Alert for persistent feedback.
- Static colors must use the token system from `styles.css`. Runtime theme values live in `lib/color-theme.ts`, mirror shadcn Create, and are applied through one injected `:root`/`.dark` variable sheet. No hard-coded Tailwind color classes.
- Runtime component styles use the source-identical shadcn style sheets in `styles/shadcn`, neutral `cn-*` hooks in shared primitives, and exactly one managed `style-*` class on `document.body`. Do not bake a single style's geometry back into shared primitives.
- Browser-local themes may change shadcn surface, action, focus, chart, and sidebar tokens. Sidebar emphasis aliases to the app primary so one selected theme never produces two competing accent colors. Oore-only success, warning, and info tokens keep their semantic meaning.
- Support dark mode using token-based styling only.
- Use sentence case. Prefer compact type, dividers, and whitespace over decorative card stacks or uppercase tracking.
- Use the shared `PageLayout`, `PageHeader`, collection controls, and Settings navigation contracts documented in `DESIGN.md`.
- Query-backed screens must distinguish initial loading, refresh, empty, filtered-empty, and error states; never present a failed request as an empty collection.
- Motion must be short and functional, respect reduced-motion preferences, and never become the primary source of hierarchy.

## Documentation and Governance Rules

- Internal technical docs and ADRs live in the private `oore.build` Obsidian vault.
- Every user-facing feature MUST add or update a feature note using `Feature Doc Template` in that vault.
- If code changes platform decisions or strict rules:
  - update `Platform Contract (V1)`
  - add or update the relevant feature note
  - add or update an ADR if changing a `MUST`-level rule
- GitHub Issues is the canonical tracker for bugs, features, roadmap work, and follow-ups.
- Do not commit private notes or add links or pointers to them elsewhere in the repository.

## Release Channels (Alpha/Beta/Stable)

Release automation is branch + tag driven via GitHub Actions:

- Merge to `alpha` -> cuts `vX.Y.Z-alpha.N` prerelease tags
- Merge to `beta` -> cuts `vX.Y.Z-beta.N` prerelease tags
- Merge to `stable` -> cuts `vX.Y.Z` production tags
- `master` is a playground branch (validated but not auto-tagged)

Before changing release automation, read `Release Channels (alpha / beta / stable) via GitHub Actions` in the private docs vault.

## Backend Bootstrap Direction

- Rust workspace crates:
- `crates/oored`
- `crates/oore`
- `crates/oore-contract`
- Keep `/v1/public/setup-status` non-sensitive.
- Setup mutating endpoints must be token-gated and disabled after `ready` (exception: Local Only mode may auto-complete setup on first loopback local login).

## Makefile Maintenance

- All build, test, lint, and dev commands must have a corresponding `make` target in the root `Makefile`.
- When adding new scripts or tooling, update the Makefile.
- `make validate` is the single command for the full pre-handoff checklist.

## Validation Checklist (Before Handoff)

- Run `make validate`.

## V1 Roadmap

- The implementation roadmap is `V1 Implementation Roadmap` in the private docs vault.
- Check off completed items and update gap summary after each phase.
- Track new work in GitHub Issues and add it to the appropriate roadmap phase or create a new phase.
- Roadmap does NOT override the Platform Contract — it sequences existing commitments.
