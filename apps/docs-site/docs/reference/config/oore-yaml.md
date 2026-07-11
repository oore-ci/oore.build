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
    - "--split-per-abi"
  ios:
    - "--flavor=staging"

platform_commands:                   # Optional. Override commands per platform.
  android: flutter build apk --release --split-per-abi
  ios: flutter build ipa --release

env:                                 # Optional. Environment variables for builds.
  - key: JAVA_HOME
    value: /usr/local/opt/openjdk@17

artifacts:                           # Optional. Artifact collection patterns.
  patterns:
    - "build/app/outputs/flutter-apk/*.apk"
    - "build/ios/ipa/*.ipa"
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

### commands

| Field | Type | Required | Description |
|---|---|---|---|
| `pre_build` | `string[]` | No | Commands run before the main build |
| `build` | `string[]` | Yes | Main build commands |
| `post_build` | `string[]` | No | Commands run after a successful build |

Commands are executed sequentially. If any command returns a non-zero exit code, the build fails and subsequent commands are skipped.

### platform_build_args

Each platform value is a string array appended to its default build command.

### env

| Field | Type | Required | Description |
|---|---|---|---|
| `key` | `string` | Yes | Environment variable name |
| `value` | `string` | Yes | Environment variable value |

### artifacts.patterns

An array of glob patterns matched against the build workspace. Matched files are collected as build artifacts.
Patterns are workspace-relative. Absolute paths, parent traversal, and symlink traversal are rejected. Filename-only patterns such as `*.apk` match anywhere in the workspace. Trigger and concurrency policy are configured on the pipeline through the UI/API; they are not repository YAML fields.

## Config resolution order

1. `.oore.yaml` (or `.oore.yml`) in repo root — highest priority
2. Pipeline execution config set via UI — fallback

## Flutter version resolution

1. `.fvmrc` in repo root — highest priority
2. `flutter_version` in `.oore.yaml` — fallback
3. Pipeline Flutter version setting in UI — final fallback
