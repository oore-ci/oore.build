---
status: implemented
description: 'CLI reference for oore doctor environment and signing diagnostics.'
---

# oore doctor

Run environment diagnostics for build/signing readiness.

## Synopsis

```bash
oore doctor [--json] [--platform android|ios|macos]... [--all]
```

## Flags

| Flag                    | Description                                                                    |
| ----------------------- | ------------------------------------------------------------------------------ |
| `--json`                | Print machine-readable diagnostic report                                       |
| `--platform <platform>` | Add Android, iOS, or macOS target checks; repeat the flag for multiple targets |
| `--all`                 | Run all platform checks                                                        |

## Checks

`oore doctor` reports status for:

- Core runner tools: `git`, Oore's bundled `fvm`, and the managed Flutter SDK
- Managed runner lifecycle on macOS: the boot-time system service is installed, running, and recently authenticated to the backend, with explicit warnings for a stopped or crash-looping service, a process that is up but has not authenticated successfully, and the legacy login-session runner
- Android (with `--platform android`): Java and an `ANDROID_HOME` / `ANDROID_SDK_ROOT` SDK with Platform-Tools
- iOS/macOS (with `--platform ios` or `--platform macos`): `xcode-select -p` and `xcodebuild -version`
- Apple signing and notarization readiness: job-scoped `security`/`codesign` tools and optional `xcrun notarytool`

`oore doctor` does not search the user's default keychain for signing identities.
Oore supplies build credentials through a temporary job keychain and validates
them when that build runs, so an empty personal keychain is not a runner error.

Every check is `ok`, `warning`, `missing`, or `skipped`. Warnings cover optional signing/notarization setup and do not fail the command.
When a warning or missing check has a repair action, plain output prints it on a
separate `Fix:` line and JSON returns it as `install_hint`.

## Exit codes

| Code | Meaning                                                    |
| ---- | ---------------------------------------------------------- |
| `0`  | All selected required checks passed (warnings are allowed) |
| `1`  | One or more required checks missing                        |

## Example output

```text
oore doctor -- environment checks
  [ok] git                git version 2.49.0
  [ok] flutter            Flutter 3.29.0
  [ok] runner_service     boot-time system service is installed, running, and authenticated
  [ok] xcode              Xcode 16.2 (/Applications/Xcode.app/Contents/Developer)
  [ok] apple_signing_tools job-scoped signing tools are available; build credentials are validated from each temporary keychain
All selected required checks passed.
```

## JSON output example

```json
{
  "checks": [
    {
      "name": "git",
      "status": "ok",
      "detail": "git version 2.49.0",
      "install_hint": null
    }
  ],
  "missing_count": 0,
  "warning_count": 0
}
```
