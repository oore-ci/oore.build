# oore.build

Self-hosted, Flutter-first mobile CI and internal app distribution.

## Project Contract

The current product and engineering contract is documented at:

- `docs/platform-contract.md`
- `docs/strict-guidelines.md`

## Workspace Layout

- `apps/web`: primary product web UI
- `apps/docs-site`: static documentation website (VitePress placeholder)
- `crates/oored`: daemon runtime
- `crates/oore`: operator CLI/TUI bootstrap surface
- `crates/oore-contract`: shared backend API contracts
- `docs/features`: required feature documentation entries
- `docs/templates/feature-doc-template.md`: required feature doc template

## Shared shadcn Preset

Both `apps/web` and `apps/docs-site` use the same shadcn preset URL in:

- `configs/shadcn-preset.txt`

Initialize shadcn in both repos with Base UI + Vega style + Hugeicons + amber theme:

- `bun run ui:init`

Reference one-shot create command:

- `bunx --bun shadcn@latest create --preset "https://ui.shadcn.com/init?base=base&style=vega&baseColor=neutral&theme=amber&iconLibrary=hugeicons&font=inter&menuAccent=subtle&menuColor=default&radius=none&template=start&rtl=false"`

## Frontend Scaffold

Both frontend apps are scaffolded with TanStack file-router:

- `bunx create-tsrouter-app@latest my-app --template file-router`

Docs site framework:

- `VitePress` (placeholder wired under `apps/docs-site/docs`)
- run docs locally: `bun run dev:docs`
- build docs: `bun run build:docs`

## Documentation Gate

Feature docs are validated by:

- local: `bun run docs:check`
- CI: `.github/workflows/docs-guard.yml`

## Backend Bootstrap

- check workspace: `cargo check --workspace`
- run daemon: `cargo run -p oored -- run --listen 127.0.0.1:8787`
- check setup status: `curl http://127.0.0.1:8787/v1/public/setup-status`
- run CLI: `cargo run -p oore -- setup open --ttl 15m`
