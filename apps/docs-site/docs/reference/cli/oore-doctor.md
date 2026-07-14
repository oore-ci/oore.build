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

- Core runner tools: `git`, `fvm`, and `flutter`
- Android (with `--platform android`): Java and an `ANDROID_HOME` / `ANDROID_SDK_ROOT` SDK with Platform-Tools
- iOS/macOS (with `--platform ios` or `--platform macos`): `xcode-select -p` and `xcodebuild -version`
- Optional Apple signing and notarization readiness: signing identities and `xcrun notarytool`

Every check is `ok`, `warning`, `missing`, or `skipped`. Warnings cover optional signing/notarization setup and do not fail the command.

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
  [ok] xcode              Xcode 16.2 (/Applications/Xcode.app/Contents/Developer)
  [warning] codesign_identity import an Apple development or distribution certificate before signing
1 required issue(s) found.
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
