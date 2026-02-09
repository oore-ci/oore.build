# File-First Pipeline Config and UI Fallback Execution

## Status

`ready`

## Problem

Pipelines previously treated `config_path` as a plain shell script file. There was no structured YAML schema, no file auto-detection, and no UI fallback model when config files were absent. This made execution brittle and blocked a Codemagic-style experience where repository config is authoritative but teams can still configure fallback behavior in the product.

## User Impact

- **Developers** can commit `.oore.yaml` or `.oore.yml` and get deterministic file-first execution.
- **Operators** can configure fallback platforms/commands/artifact patterns in pipeline create/edit dialogs.
- Builds now fail fast when a repo config file exists but is invalid YAML/schema, preventing silent drift.
- Existing pipelines continue to run with seeded Android fallback defaults.

## UI Changes

- Pipeline create/edit dialogs now include:
- config source mode (`auto-detect` or `explicit path`)
- platform toggles (`android`, `ios`, `macos`)
- optional `flutter_version` override
- staged fallback command editors (`pre_build`, `build`, `post_build`)
- artifact pattern editor
- default command preview generated from selected platforms
- Pipeline detail page now shows config resolution mode and fallback execution config summary.

## API Changes

Contract additions:

- `BuildPlatform`: `android | ios | macos`
- `PipelineCommandStages`: `pre_build[]`, `build[]`, `post_build[]`
- `PipelineExecutionConfig`: `platforms`, `commands`, `artifact_patterns`
  - plus `flutter_version`, `platform_build_args`, `platform_commands`, and `env`

Pipeline payload additions:

- `config_path_explicit: bool`
- `execution_config: PipelineExecutionConfig`

Extended request payloads:

- `CreatePipelineRequest`
- `UpdatePipelineRequest`
- `ValidatePipelineRequest`

Runtime snapshot changes:

- Build `config_snapshot` now uses `snapshot_version = 2`
- Adds `config_resolution_policy = "file_first_ui_fallback"`
- Adds immutable `config_path_explicit` and `ui_execution_config`

Execution resolution order in runner:

1. If `config_path_explicit = true`, check only explicit path.
2. Else auto-detect `.oore.yaml`, then `.oore.yml`.
3. If file exists, parse strict YAML schema and execute file config.
4. If file missing, execute UI fallback config from immutable snapshot.
5. If file exists but invalid, fail build immediately.

Flutter toolchain resolution:

1. If repo contains `.fvmrc`, use that version.
2. Else use `execution_config.flutter_version` when configured in UI/API.
3. When a version is resolved, runner prepends `fvm use <version> --force` and executes Flutter/Dart commands via `fvm`.

## Security Considerations

- Invalid YAML/schema now fails fast; no silent fallback when repository config is malformed.
- Snapshot includes immutable fallback execution settings captured at build creation time.
- Existing RBAC boundaries for pipeline create/update/validate remain unchanged.
- Command validation rejects empty stage commands and malformed artifact globs before persistence.

## Migration and Rollout

- Added migration `009_pipeline_execution_config.sql`:
- `pipelines.config_path_explicit` (default `0`)
- `pipelines.execution_config` (default `{}`)
- Backfill existing rows with Android fallback defaults:
  - platforms: `["android"]`
  - commands: empty arrays
  - artifact_patterns: `["*.apk"]`
- No endpoint removals; changes are additive and backward-compatible.

## Acceptance Criteria

- [x] Runner resolves `.oore.yaml`/`.oore.yml` file-first with explicit-path precedence.
- [x] Missing repo config uses UI fallback execution config from snapshot.
- [x] Invalid repo config fails build immediately.
- [x] Pipeline create/edit UI supports platform toggles and staged custom commands.
- [x] Flutter version can be controlled by `.fvmrc` or pipeline fallback config (`flutter_version`).
- [x] Build snapshots capture immutable fallback config and resolution metadata (`snapshot_version = 2`).
- [x] Automated checks pass (`cargo test`, `bun test`, `bun build`).

## Owner

oore.build team

## Last Updated

`2026-02-09`
