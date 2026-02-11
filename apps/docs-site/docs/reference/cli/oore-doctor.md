---
status: implemented
---

# oore doctor

Verify that all required development tools are installed.

## Synopsis

```bash
oore doctor
```

## Description

Checks for the presence and version of every tool required to build Flutter projects with oore.build. Returns exit code `0` if all tools are found, or `1` if any are missing.

## Checks

| Tool | Check command | Install hint |
|------|---------------|--------------|
| `git` | `git --version` | `brew install git` |
| `rustc` | `rustc --version` | `curl https://sh.rustup.rs -sSf \| sh` |
| `cargo` | `cargo --version` | `curl https://sh.rustup.rs -sSf \| sh` |
| `bun` | `bun --version` | `curl -fsSL https://bun.sh/install \| bash` |
| `fvm` | `fvm --version` | `brew tap leoafarias/fvm && brew install fvm` |
| `flutter` | `flutter --version` | `fvm install <version> && fvm use <version>` |
| `xcodebuild` | `xcodebuild -version` | `xcode-select --install` |

## Example output

```
Checking development environment...

  [ok]      git 2.44.0
  [ok]      rustc 1.82.0
  [ok]      cargo 1.82.0
  [ok]      bun 1.1.38
  [ok]      fvm 3.2.1
  [ok]      flutter 3.24.5
  [missing] xcodebuild — install with: xcode-select --install

1 issue found.
```

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | All tools found |
| `1` | One or more tools missing |

## When to run

- After a fresh macOS install, before setting up the daemon
- When builds fail with "command not found" errors
- As part of the [prerequisites check](/getting-started/prerequisites) during initial setup
