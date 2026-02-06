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

## Setup Flow

### 1. Start the daemon
```bash
make run-daemon
```

### 2. Open a setup window
```bash
# Generate a bootstrap token valid for 15 minutes
oore setup open --ttl 15m

# Or with JSON output for scripts
oore setup open --ttl 15m --json
```

### 3. Complete setup via API
```bash
# Verify bootstrap token (one-time use)
curl -X POST http://127.0.0.1:8787/v1/setup/bootstrap-token/verify \
  -H "Content-Type: application/json" \
  -d '{"token": "<bootstrap-token>"}'

# Configure OIDC provider
curl -X POST http://127.0.0.1:8787/v1/setup/oidc/configure \
  -H "Authorization: Bearer <session-token>" \
  -H "Content-Type: application/json" \
  -d '{"issuer_url": "https://accounts.google.com", "client_id": "...", "client_secret": "..."}'

# Start owner OIDC verification
curl -X POST http://127.0.0.1:8787/v1/setup/owner/start-oidc \
  -H "Authorization: Bearer <session-token>" \
  -H "Content-Type: application/json" \
  -d '{"redirect_uri": "http://127.0.0.1:3000/setup/owner/callback"}'

# Verify owner OIDC (after completing the OIDC flow)
curl -X POST http://127.0.0.1:8787/v1/setup/owner/verify-oidc \
  -H "Authorization: Bearer <session-token>" \
  -H "Content-Type: application/json" \
  -d '{"code": "<authorization_code>", "state": "<state>"}'

# Complete setup
curl -X POST http://127.0.0.1:8787/v1/setup/complete \
  -H "Authorization: Bearer <session-token>"
```

### 4. Check status
```bash
curl http://127.0.0.1:8787/v1/public/setup-status
```
