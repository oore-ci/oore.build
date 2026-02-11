# Release Installer and Neutral Landing Site

## Status

`ready`

## Problem

Tester onboarding required cloning the repository and building from source, which added unnecessary setup time and local toolchain complexity for first-time evaluation. There was also no neutral root landing page to present installation and hosted UI entrypoints.

## User Impact

Operators and testers can now install prebuilt macOS binaries using a single command:

`curl -fsSL https://oore.build/install | bash`

They are guided to complete setup from the hosted UI (`https://ci.oore.build`) and docs (`https://docs.oore.build`) without requiring local frontend hosting.

## UI Changes

- Added a dedicated static landing site (`apps/site`) for `oore.build`.
- Landing page provides:
  - one-line product description
  - install command
  - links to hosted UI, docs, and repository
  - explicit V1 runtime note (macOS backend)

## API Changes

No backend HTTP API shape changes.

Release/distribution contract additions:

- Release assets (hosted on `dl.oore.build`):
  - `oore_<version>_darwin_arm64.tar.gz`
  - `oore_<version>_darwin_x86_64.tar.gz`
  - `oore_<version>_checksums.txt`
- Tarballs contain `bin/oored`, `bin/oore`, `VERSION`, and `LICENSE`.

## Security Considerations

- Installer verifies SHA-256 checksums before installing binaries.
- Installer fails closed on missing release assets, invalid checksums, or unsupported architecture.
- `latest` resolution is manifest-driven via `https://dl.oore.build/releases/latest.json`.
- Hosted UI onboarding remains aligned with contract boundaries (`ci.oore.build` is UI-only; customer backend executes builds).

## Migration and Rollout

- Installer path moved from source-build bootstrap to release-artifact bootstrap.
- Added local macOS release automation scripts for semver tags (`v*.*.*`) that package both architectures and upload to Cloudflare R2.
- Added webhook-based release automation service with launchd templates for both LaunchDaemon (system) and LaunchAgent (user) operation.
- LaunchDaemon mode runs under the build user and writes webhook logs to the user log directory for reliable startup (`~/Library/Logs/oore-release-webhook.log`).
- Added site build/deploy plumbing (`dev-site`, `build-site`, `deploy-site`) and root scripts (`dev:site`, `build:site`).

## Acceptance Criteria

- [x] `scripts/install.sh` installs release binaries from `dl.oore.build` and verifies checksums.
- [x] Installer defaults to interactive prompts and supports non-interactive env-based mode.
- [x] `apps/site` provides a neutral landing page and serves `/install`.
- [x] Local release automation builds and publishes macOS arm64 + x86_64 artifacts plus checksums.
- [x] Docs reflect release-based install flow and hosted UI onboarding.

## Owner

Core platform

## Last Updated

`2026-02-11`
