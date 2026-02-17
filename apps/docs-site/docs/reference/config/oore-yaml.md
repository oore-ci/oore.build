---
status: implemented
description: "Complete reference for the .oore.yaml pipeline configuration file format."
---

# .oore.yaml Reference

The `.oore.yaml` file (or `.oore.yml`) defines pipeline execution configuration for an Oore CI project. Place it in the root of your repository.

For a guide on writing this file, see [Write a Pipeline Config](/guides/projects/pipeline-config).

## Schema

```yaml
version: 1                          # Required. Must be 1.

flutter_version: "3.24.0"           # Optional. Overridden by .fvmrc if present.

platforms:                           # Required. At least one platform.
  - android                          # Options: android, ios, macos
  - ios

commands:                            # Required. Build command stages.
  pre_build:                         # Optional. Runs before build.
    - flutter pub get
    - dart run build_runner build
  build:                             # Required. Main build commands.
    - flutter build apk --release
  post_build:                        # Optional. Runs after successful build.
    - echo "Build complete"

platform_build_args:                 # Optional. Per-platform build arguments.
  android:
    extra_args: "--split-per-abi"
  ios:
    export_method: "ad-hoc"          # Options: ad-hoc, app-store, development, enterprise

platform_commands:                   # Optional. Override commands per platform.
  android:
    build:
      - flutter build apk --release --split-per-abi
  ios:
    build:
      - flutter build ipa --release --export-method ad-hoc

env:                                 # Optional. Environment variables for builds.
  - key: JAVA_HOME
    value: /usr/local/opt/openjdk@17

artifacts:                           # Optional. Artifact collection patterns.
  patterns:
    - "**/*.apk"
    - "**/*.ipa"

triggers:                            # Optional. Webhook trigger configuration.
  events:
    - push                           # Options: push, pull_request
    - pull_request
  branches:
    - main
    - "release/*"                    # Supports glob patterns

concurrency:                         # Optional. Concurrency controls.
  max_concurrent: 1                  # Max simultaneous builds for this pipeline.
  cancel_in_progress: true           # Cancel older builds when a new one starts.
```

## Field reference

### Top level

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `version` | `integer` | Yes | — | Schema version. Must be `1`. |
| `flutter_version` | `string` | No | — | Flutter SDK version. Overridden by `.fvmrc` in repo root. |
| `platforms` | `string[]` | Yes | — | Target platforms: `android`, `ios`, `macos`. |
| `commands` | `object` | Yes | — | Build command stages. |
| `platform_build_args` | `object` | No | — | Per-platform build arguments. |
| `platform_commands` | `object` | No | — | Per-platform command overrides. |
| `env` | `object[]` | No | `[]` | Environment variables. |
| `artifacts` | `object` | No | — | Artifact collection config. |
| `triggers` | `object` | No | — | Webhook trigger rules. |
| `concurrency` | `object` | No | — | Concurrency controls. |

### commands

| Field | Type | Required | Description |
|---|---|---|---|
| `pre_build` | `string[]` | No | Commands run before the main build |
| `build` | `string[]` | Yes | Main build commands |
| `post_build` | `string[]` | No | Commands run after a successful build |

Commands are executed sequentially. If any command returns a non-zero exit code, the build fails and subsequent commands are skipped.

### platform_build_args

| Platform | Field | Type | Description |
|---|---|---|---|
| `android` | `extra_args` | `string` | Extra arguments passed to the Android build command |
| `ios` | `export_method` | `string` | IPA export method: `ad-hoc`, `app-store`, `development`, `enterprise` |

### env

| Field | Type | Required | Description |
|---|---|---|---|
| `key` | `string` | Yes | Environment variable name |
| `value` | `string` | Yes | Environment variable value |

### artifacts.patterns

An array of glob patterns matched against the build workspace. Matched files are collected as build artifacts.

### triggers

| Field | Type | Required | Description |
|---|---|---|---|
| `events` | `string[]` | No | Trigger events: `push`, `pull_request` |
| `branches` | `string[]` | No | Branch filters (supports glob patterns) |

### concurrency

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `max_concurrent` | `integer` | No | `1` | Maximum simultaneous builds |
| `cancel_in_progress` | `boolean` | No | `false` | Cancel running builds when a new one is triggered |

## Config resolution order

1. `.oore.yaml` (or `.oore.yml`) in repo root — highest priority
2. Pipeline execution config set via UI — fallback

## Flutter version resolution

1. `.fvmrc` in repo root — highest priority
2. `flutter_version` in `.oore.yaml` — fallback
3. Pipeline Flutter version setting in UI — final fallback
