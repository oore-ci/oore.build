# Build Isolation

## Status

`ready`

## Problem

Build execution must be isolated to prevent interference between concurrent builds and ensure deterministic cleanup. Without isolation, concurrent builds on the same runner can corrupt each other's state through shared directories, environment variables, or leftover files.

## User Impact

- **Developers** get deterministic, clean workspaces for every build.
- **Operators** do not need manual workspace cleanup after failures.
- **Admins** keep single-tenant process isolation guarantees in V1.
- **Pipeline authors** get deterministic execution planning:
  - repository config (`.oore.yaml` / `.oore.yml`) is authoritative when present
  - UI fallback config is used only when repository config is absent

## UI Changes

No direct isolation UI changes. Build detail and pipeline detail pages now expose execution source policy and fallback config summary.

## API Changes

Build runtime consumes immutable `config_snapshot` captured at build creation time.

Snapshot v2 includes:

- `snapshot_version: 2`
- `config_resolution_policy: file_first_ui_fallback`
- `config_path`
- `config_path_explicit`
- `ui_execution_config`
- trigger metadata (`trigger_type`, `commit_sha`, `branch`, `repo_url`, `captured_at`)

Runner execution resolution:

1. If `config_path_explicit=true`, check explicit path only.
2. Else check `.oore.yaml`, then `.oore.yml`.
3. If file exists, parse strict YAML schema and execute it.
4. If file missing, execute `ui_execution_config` fallback from snapshot.
5. If file exists but is invalid, fail build immediately (no silent fallback).
6. Flutter version resolution: `.fvmrc` in repo wins; otherwise use `ui_execution_config.flutter_version`.

Staged execution model (V1):

- `pre_build`
- `build`
- `post_build`

Default Flutter commands are generated per selected platform, and custom build commands run after defaults.
When Flutter version is resolved, runner executes `fvm use <version> --force` first and runs Flutter/Dart commands through `fvm`.

## Security Considerations

- **Process-level isolation**: each build runs in its own child process tree.
- **Dedicated workspace**: `/tmp/oore-builds/{build_id}`.
- **Deterministic cleanup**: workspace removed at end of run (including failures/cancellations).
- **Fail-fast config validation**: malformed repo config fails build immediately, preventing accidental fallback to stale UI settings.
- **Immutable snapshot**: fallback execution settings are captured at build creation time and cannot drift mid-run.

## Migration and Rollout

- Isolation model remains runner-side with no breaking runtime contract changes.
- Build snapshot format moved from v1 to v2 (additive fields).
- Existing pipelines receive seeded fallback config defaults via migration 009.

## Acceptance Criteria

- [x] Each build uses a unique ephemeral workspace under `/tmp/oore-builds/{build_id}`.
- [x] Checkout uses commit-pinned fetch when `commit_sha` is present, branch fallback otherwise.
- [x] Runner resolves config source as file-first with UI fallback only when file is absent.
- [x] Invalid repository YAML fails the build immediately.
- [x] Stage results are captured with stage-aware names (`pre_build-*`, `build-*`, `post_build-*`).
- [x] Workspace cleanup happens on success/failure/cancel paths.

## Owner

oore.build team

## Last Updated

`2026-02-09`
