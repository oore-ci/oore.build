# File-First Pipeline Config

oore.build uses a **file-first** execution model with **UI fallback**.

## Resolution order

For every build, the runner resolves pipeline config in this order:

1. If pipeline is set to explicit mode, check only that explicit path.
2. Otherwise check `.oore.yaml`, then `.oore.yml`.
3. If a file is found, parse and execute that file.
4. If no file exists, use the immutable fallback config captured from the pipeline UI.
5. If a file exists but is invalid, the build fails immediately.

## Supported YAML schema (v1)

```yaml
version: 1
flutter_version: 3.24.0
platforms:
  - android
  - ios
commands:
  pre_build:
    - flutter pub get
  build:
    - flutter build apk --release
  post_build:
    - echo "done"
artifacts:
  patterns:
    - "*.apk"
    - "*.ipa"
```

Supported platforms: `android`, `ios`, `macos`.

## UI fallback behavior

In pipeline create/edit dialogs, you can define fallback:

- platform toggles
- custom commands for `pre_build`, `build`, `post_build`
- artifact patterns

When fallback is used, default Flutter commands run first (per selected platforms), then your custom `build` commands.

Flutter version selection:

1. `.fvmrc` in repo has highest priority.
2. If `.fvmrc` is missing, use pipeline fallback `flutter_version` (if set).
3. When a version is resolved, runner runs `fvm use <version> --force` and executes Flutter/Dart commands with `fvm`.

## Notes

- Snapshot metadata is immutable per build (`snapshot_version = 2`).
- Fallback is only used when no repo config file is found.
- Invalid repo config does not fall back silently.
- `fvm` is required on runner hosts.
