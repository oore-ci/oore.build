# Dev Data Isolation and Cleanup Target

## Status

`ready`

## Problem

Running source development daemon commands and installed production daemon flows on the same macOS host reused the same default data locations. This created risk of overwriting setup state, artifact paths, and runtime key files during local testing.

## User Impact

Developers/operators can now run development builds without colliding with production data on the same machine. They also get a single cleanup target to reset dev state for repeated end-to-end setup testing.

## UI Changes

No web UI changes.

## API Changes

No HTTP endpoint shape changes.

Runtime/config behavior changes:

- Added daemon data root override: `OORED_DATA_DIR` (with `OORE_DATA_DIR` alias fallback).
- Daemon default path resolution now consistently derives:
  - SQLite DB (`oore.db`)
  - encryption key file (`encryption.key`) for file mode/fallback
  - local artifact storage default directory (`artifacts`)
  from the same resolved data root.

## Security Considerations

- Isolation reduces accidental cross-environment data reuse.
- Production data is not touched by `make run-daemon*` defaults.
- Cleanup target removes only dev data root by default (`~/.oore/dev`).

## Migration and Rollout

- `make run-daemon`, `make run-daemon-debug`, and `make run-daemon-release` now set:
  - `OORED_DATA_DIR=$(HOME)/.oore/dev`
- `make run-cli` now sets:
  - `OORE_SETUP_STATE_FILE=$(HOME)/.oore/dev/oore.db`
- Added `make clean-dev-state` to remove that dev root.
- Existing production deployments are unaffected unless `OORED_DATA_DIR`/`OORE_DATA_DIR` is explicitly set.

## Acceptance Criteria

- [x] Dev daemon runs use isolated data root by default through Makefile.
- [x] Local setup token generation (`make run-cli`) writes to isolated dev DB by default.
- [x] Daemon path defaults (db/key/local-artifacts) resolve through shared data-root logic.
- [x] Dev cleanup target removes isolated dev data only.
- [x] Docs include new env var and dev cleanup workflow.

## Owner

Core platform

## Last Updated

`2026-02-12`
