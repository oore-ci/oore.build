---
status: implemented
description: "Configure build pipelines using .oore.yaml in your repository."
---

# Write a Pipeline Config (.oore.yaml)

The `.oore.yaml` file defines how Oore CI builds your Flutter application. Place it in the root of your repository — Oore CI reads it at build time.

## What you need

- A [project](/guides/projects/create-project) in Oore CI
- A Flutter repository

## Minimal example

```yaml
version: 1
flutter_version: "3.24.0"
platforms:
  - android
commands:
  pre_build:
    - flutter pub get
  build:
    - flutter build apk --release
  post_build: []
artifacts:
  patterns:
    - "build/app/outputs/flutter-apk/*.apk"
```

## Full schema

For the complete `.oore.yaml` schema reference, see [.oore.yaml Reference](/reference/config/oore-yaml).

### Top-level fields

| Field | Type | Required | Description |
|---|---|---|---|
| `version` | `integer` | Yes | Schema version (must be `1`) |
| `flutter_version` | `string` | No | Flutter version to use. Overridden by `.fvmrc` if present. |
| `platforms` | `string[]` | Yes | Target platforms: `android`, `ios`, `macos` |
| `commands` | `object` | Yes | Build command stages |
| `artifacts` | `object` | No | Artifact collection patterns |

### Commands

Commands are executed in three stages:

| Stage | When it runs | Typical use |
|---|---|---|
| `pre_build` | Before the main build | `flutter pub get`, code generation |
| `build` | Main build step | `flutter build apk`, `flutter build ipa` |
| `post_build` | After a successful build | Testing, additional processing |

Each stage is a list of shell commands executed sequentially. If any command returns a non-zero exit code, the build fails.

### Platform-specific build args

```yaml
platform_build_args:
  android:
    - "--split-per-abi"
  ios:
    - "--flavor=staging"
```

### Platform-specific commands

Override the default build command per platform:

```yaml
platform_commands:
  android: flutter build apk --release --split-per-abi
  ios: flutter build ipa --release
```

### Environment variables

```yaml
env:
  - key: JAVA_HOME
    value: /usr/local/opt/openjdk@17
  - key: MY_BUILD_VAR
    value: custom-value
```

### Artifact patterns

```yaml
artifacts:
  patterns:
    - "build/app/outputs/flutter-apk/*.apk"
    - "build/ios/ipa/*.ipa"
    - "build/macos/Build/Products/Release/*.app"
```

Glob patterns are matched against workspace-relative paths. Filename-only patterns such as `*.apk` still match anywhere. Configure triggers and concurrency in the pipeline UI/API, not in repository YAML.

## Multi-platform example

```yaml
version: 1
flutter_version: "3.24.0"
platforms:
  - android
  - ios
commands:
  pre_build:
    - flutter pub get
    - dart run build_runner build --delete-conflicting-outputs
  build:
    - flutter build apk --release
    - flutter build ipa --release --export-method ad-hoc
  post_build: []
platform_build_args:
  android:
    extra_args: "--split-per-abi"
artifacts:
  patterns:
    - "build/app/outputs/flutter-apk/*.apk"
    - "build/ios/ipa/*.ipa"
```

## Config resolution

Oore CI resolves pipeline configuration in this order:

1. `.oore.yaml` (or `.oore.yml`) in the repository root — **highest priority**
2. Pipeline execution config set via the UI — **fallback**

If both exist, the file takes precedence.

For Flutter version resolution:

1. `.fvmrc` in the repository — **highest priority**
2. `flutter_version` in `.oore.yaml` — **fallback**
3. Pipeline's Flutter version setting in the UI — **final fallback**

## Validation

Validate a pipeline config locally with the same parser used by the runner:

```bash
oore pipeline validate .oore.yaml
```
