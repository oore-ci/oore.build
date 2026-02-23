---
status: implemented
description: "CLI reference for oore doctor environment and signing diagnostics."
---

# oore doctor

Run environment diagnostics for build/signing readiness.

## Synopsis

```bash
oore doctor [--json]
```

## Flags

| Flag | Description |
|------|-------------|
| `--json` | Print machine-readable diagnostic report |

## Checks

`oore doctor` reports status for:

- Core tooling: `git`, `rustc`, `cargo`, `bun`, `fvm`, `flutter`
- Xcode CLI readiness:
  - `xcodebuild -version`
  - `xcode-select -p`
- Code signing identities:
  - `security find-identity -v -p codesigning`
- Notarization tooling:
  - `xcrun notarytool --version`

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | All required checks passed |
| `1` | One or more required checks missing |

## Example output

```text
oore doctor -- environment checks
  [ok] git                git version 2.49.0
  [ok] rustc              rustc 1.86.0
  [ok] xcode_cli          xcodebuild + xcode-select configured
  [missing] codesign_identity install: import a Developer/Application certificate into Keychain Access
1 issue(s) found.
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
  "missing_count": 0
}
```
