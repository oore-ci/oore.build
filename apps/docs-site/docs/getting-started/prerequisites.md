---
status: implemented
description: 'System requirements for running Oore CI including macOS, Xcode, and Flutter dependencies.'
---

# Prerequisites

Before installing Oore CI, verify that your system meets these requirements.

## System requirements

| Requirement                  | Details                                                                                     |
| ---------------------------- | ------------------------------------------------------------------------------------------- |
| **Operating system**         | macOS (required for daemon and CLI in V1)                                                   |
| **Network access**           | Reachability to GitHub (`github.com`), your OIDC provider, and your source control provider |
| **Xcode Command Line Tools** | Required for iOS/macOS build jobs (`xcode-select --install`)                                |

## Required tools for release installer

The one-line installer (`curl -fsSL https://oore.build/install | bash`) requires:

- `curl`
- `tar`
- `shasum`

macOS includes these by default.

## Required tools for running Flutter build jobs

After install, runners/build hosts should have:

- `git`
- `fvm`
- `flutter`
- `xcodebuild`

Install FVM via Homebrew:

```bash
brew tap leoafarias/fvm
brew install fvm
```

Verify:

```bash
fvm --version
flutter --version
xcodebuild -version
```

## Optional tools for source development

If you plan to build Oore CI from source (instead of using release binaries), install:

- Rust toolchain (`rustup`, Rust 1.85+)
- Bun (frontend package manager/runtime)

## OIDC provider account

Oore CI does not support local passwords.

If you plan to use Remote mode with `remote_auth_mode=oidc` (default), you will need an OIDC provider:

- **Issuer URL**
- **Client ID**
- **Client secret** (if required by your provider)

If you don't have an OIDC provider configured yet, see [Configure OIDC](/guides/oidc/).

## Quick check

Run diagnostics after installation:

```bash
oore doctor
```

`oore doctor` checks the runner runtime by default. Add `--platform android`, `--platform ios`, or `--platform macos` (repeatable), or use `--all`, for target-specific requirements. Rust and Bun are source-development tools and are not release-runner requirements.

## Next step

[Install Oore CI](/getting-started/install)
