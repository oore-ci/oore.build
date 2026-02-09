# Pipelines CRUD API and Configuration Validation

## Status

`ready`

## Problem

Pipeline configuration needed a stable API surface and validation rules for both trigger/concurrency policy and execution fallback settings. Without strict validation, invalid platform/command configs and malformed artifact patterns could reach runtime and fail late.

## User Impact

- **Developers** can create and update pipelines with validated trigger policy and fallback execution config.
- **Operators** can choose config mode (`auto-detect` vs explicit path) per pipeline.
- **Teams** can define fallback platforms and staged commands in UI/API when repo config files are absent.
- **Validation endpoint** catches invalid trigger, concurrency, path-policy, and execution-config inputs before save.

## UI Changes

- Pipeline create/edit dialogs now include:
- config source mode selector (`auto-detect` / `explicit path`)
- fallback platform toggles (`android`, `ios`, `macos`)
- custom staged command editors (`pre_build`, `build`, `post_build`)
- artifact pattern editor
- Pipeline detail shows config resolution mode and fallback execution summary.

## API Changes

Pipeline endpoints remain the same but payloads are extended.

Endpoints:

- `POST /v1/projects/{project_id}/pipelines`
- `GET /v1/projects/{project_id}/pipelines`
- `GET /v1/pipelines/{pipeline_id}`
- `PATCH /v1/pipelines/{pipeline_id}`
- `DELETE /v1/pipelines/{pipeline_id}`
- `POST /v1/pipelines/validate`

New pipeline fields:

- `config_path_explicit: bool`
- `execution_config: { platforms, flutter_version?, commands, platform_build_args?, platform_commands?, env?, artifact_patterns }`

New execution config schema:

- `platforms`: non-empty subset of `android | ios | macos`
- `commands.pre_build[]`
- `commands.build[]`
- `commands.post_build[]`
- `artifact_patterns[]` (extension globs like `*.apk`)

Validation rules:

- Trigger events must be one of `push | pull_request | tag_push`
- Branch patterns must be non-empty strings when provided
- `max_concurrent` must be `1..100` when provided
- `config_path` is required when `config_path_explicit=true`
- `execution_config.platforms` must be non-empty
- stage commands must be non-empty strings
- `execution_config.flutter_version` must be non-empty when provided (max 64 chars)
- artifact patterns must be extension globs (`*.ext`)

## Security Considerations

- RBAC remains enforced per endpoint (`pipelines:read/write/delete`).
- All create/update/delete actions remain auditable.
- Input validation prevents malformed execution config from reaching the runner.
- Pipeline delete remains blocked for non-terminal builds (`409 active_builds`).

## Migration and Rollout

- Added migration `009_pipeline_execution_config.sql` with additive fields:
- `config_path_explicit`
- `execution_config`
- Existing rows are backfilled with Android fallback defaults.
- No endpoint removals or breaking path changes.

## Acceptance Criteria

- [x] Pipeline CRUD accepts and returns execution config fields.
- [x] Validation endpoint rejects invalid execution configs with structured errors.
- [x] Config-path explicit mode is enforced by API validation.
- [x] Existing trigger/concurrency validation behavior is preserved.
- [x] Delete semantics for active builds remain unchanged.

## Owner

Phase 5 team

## Last Updated

`2026-02-09`
