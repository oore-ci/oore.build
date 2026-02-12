# Dev Fresh Setup and State Isolation

## Status

`ready`

## Problem

Local development setup flows reused production-default macOS data paths (`~/Library/Application Support/oore`), which made it easy to accidentally reuse or overwrite real instance state while testing setup changes. There was no single command to repeatedly simulate a clean first-run setup from source.

## User Impact

Developers can now run a repeatable "from scratch" simulation using isolated dev state under `~/.oore/dev`, without touching production-default daemon data. This makes setup-flow validation faster and safer on machines that also run production-like installs.

## UI Changes

No product UI changes.

## API Changes

No HTTP API surface changes.

Developer/runtime contract additions:

- `OORED_DATA_DIR` and `OORE_DATA_DIR` now influence daemon default paths for:
  - setup DB (`oore.db`)
  - encryption key (`encryption.key`)
  - local artifacts directory (`artifacts/`)
- New developer automation script:
  - `scripts/dev-fresh-setup.sh`
  - script auto-starts a Cloudflare quick tunnel and prints the assigned public URL
- defaults to token-only setup so hosted UI E2E remains the primary setup path (no CLI OIDC by default)
  - defaults dev data root to `~/.oore/dev.noindex` and writes `.metadata_never_index` to avoid Spotlight indexing churn
- New make targets:
  - `make clean-dev-state`
  - `make dev-fresh-setup`
  - cleanup now stops matching dev daemon + tunnel processes before deleting dev state

## Security Considerations

- Dev cleanup is restricted to an explicit dev directory and includes basic guards to avoid unsafe root/home deletions.
- Isolating dev and production data reduces accidental cross-environment secret/key reuse.
- Existing auth/bootstrap invariants are unchanged.

## Migration and Rollout

- No data migration required.
- Existing installs keep current behavior unless `OORED_DATA_DIR`/`OORE_DATA_DIR` are set.
- Contributor workflow:
  1. `make clean-dev-state`
  2. `make dev-fresh-setup`

## Acceptance Criteria

- [x] Daemon default data paths support isolated roots via env vars.
- [x] `make run-daemon*` and `make run-cli` use isolated dev paths by default.
- [x] `make clean-dev-state` removes dev-only state.
- [x] `make dev-fresh-setup` performs clean state reset, local build, daemon start, and setup flow bootstrap.
- [x] `make dev-fresh-setup` starts a Cloudflare tunnel by default and prints the assigned public domain.
- [x] `make dev-fresh-setup` defaults to token-only setup for UI E2E (`OORE_DEV_SETUP_MODE=token`).

## Owner

Core platform

## Last Updated

`2026-02-12`
