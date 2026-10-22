<p align="center">
  <img src=".github/logo.svg" alt="Oore CI" width="80" height="80" />
</p>

<h1 align="center">Oore CI</h1>

<p align="center">Self-hosted, Flutter-first mobile CI and internal app distribution platform.</p>

<p align="center">
  <a href="https://docs.oore.build">Documentation</a>
  ·
  <a href="https://demo.oore.build">Live Demo</a>
  ·
  <a href="https://ci.oore.build">Hosted UI</a>
</p>

<p align="center">
  <a href="https://zerodha.tech"><img src="https://zerodha.tech/static/images/github-badge.svg" /></a>
  <br />
  <a href="https://github.com/oore-ci/oore.build/actions/workflows/validate.yml"><img src="https://github.com/oore-ci/oore.build/actions/workflows/validate.yml/badge.svg?branch=master" alt="CI" /></a>
</p>

> **Alpha** — APIs, configuration formats, and CLI flags can change before the stable release.

## Overview

Oore CI lets you run your own mobile CI server. V1 targets Android, iOS, and macOS Flutter builds on a macOS host. It provides:

- A daemon (`oored`) that runs builds and serves the API.
- An operator CLI (`oore`) for setup, administration, and runner management.
- A web UI for builds, projects, releases, and access control.
- OIDC authentication for non-loopback access.
- Local login for loopback-only access.

## Screenshots

| Dashboard                                                     | Builds                                                  |
| ------------------------------------------------------------- | ------------------------------------------------------- |
| ![Oore CI dashboard](shared/media/product/demo-dashboard.png) | ![Oore CI builds](shared/media/product/demo-builds.png) |

## Prerequisites

- macOS on `arm64` or `x86_64` for the guided v0.1.42 installation
- `curl`, `tar`, `ssh-keygen`, and the macOS system `/usr/bin/stat`, `/usr/bin/lockf`, and `/usr/bin/shasum`

For source development, also install [Rust](https://rustup.rs/) and [Bun](https://bun.sh/).

Published Linux web archives are standalone assets. They are not a guided Web node installation in v0.1.42.

## Quick Start

```bash
# Install the latest stable Oore CLI
curl -fsSL https://oore.build/install | bash

# Choose this device role and continue to setup
~/.oore/bin/oore install
```

The download script verifies a signed release manifest and installs only the `oore` CLI. The guided command then offers five device roles:

| Role          | Purpose                                                          |
| ------------- | ---------------------------------------------------------------- |
| Complete      | Run the control plane, local web UI, and builds on this Mac.     |
| Control plane | Manage pipelines and runners without a local web UI or runner.   |
| Runner        | Run builds for a control plane on another Mac.                   |
| Web node      | Serve the web UI for a control plane on another Mac.             |
| CLI only      | Connect the CLI to a ready control plane without local services. |

Interactive installation continues to `oore setup`. SSH sessions use terminal setup. Local Complete installations use browser setup by default.

Setup and support:

- [Start with Oore](https://docs.oore.build/start)
- [Report an issue](https://docs.oore.build/operate/support/report-an-issue)
- [Known limitations](https://docs.oore.build/operate/known-limitations)
- [Live demo](https://demo.oore.build)

Install prerelease channels:

```bash
# Bootstrap the CLI from the latest alpha
curl -fsSL https://oore.build/install | OORE_CHANNEL=alpha bash
~/.oore/bin/oore install

# Bootstrap the CLI from the latest beta
curl -fsSL https://oore.build/install | OORE_CHANNEL=beta bash
~/.oore/bin/oore install
```

Check for an update:

```bash
oore update --check
```

`oore update` now updates Ready Complete and Control plane installations in place. It drains active runner work, creates a backup, restarts managed services, and restores the prior release if verification fails.

Use `oore-web update` on a separate Web node. Other profiles do not contain a managed backend.

See [Upgrade Oore](https://docs.oore.build/operate/maintain/upgrade) for the complete update procedure.

## Development (from source)

```bash
bun install

make clean-dev-state
make run-daemon
make setup-token
make dev-web
```

Notes:

- `make dev-fresh-setup` resets development state, builds the project, and starts the daemon.
- It also starts a Cloudflare quick tunnel and prints the public URL.
- Disable the tunnel with `OORE_DEV_ENABLE_TUNNEL=0 make dev-fresh-setup`.
- The default setup mode uses a token for the hosted UI.
- Use `OORE_DEV_SETUP_MODE=cli make dev-fresh-setup` for CLI-based OIDC setup.
- Development commands store state in `~/.oore/dev.noindex`.

## Project Structure

```
apps/web/           React 19 + TanStack Router (product UI)
apps/docs/          Astro 7 + Fumadocs static documentation
apps/site/          Neutral landing/install site (`oore.build`)
crates/oored/       Daemon — Axum HTTP server
crates/oore/        Operator CLI — Clap
crates/oore-runner/ Build runner agent
crates/oore-contract/ Shared data types (Serde structs)
```

## Releases

Releases are published via GitHub Actions.

- A merge to `alpha` creates a `vX.Y.Z-alpha.N` prerelease.
- A merge to `beta` creates a `vX.Y.Z-beta.N` prerelease.
- A merge to `stable` creates a `vX.Y.Z` release.
- A tag builds macOS artifacts, deploys the public sites, and publishes a GitHub release.

The release workflow requires two protected secrets:

- `OORE_RELEASE_SIGNING_KEY` contains the Ed25519 private key.
- `OORE_RELEASE_PUBLICATION_TOKEN` has `Administration: read`, `Contents: write`, and `Workflows: write` for this repository only.

The Ed25519 public key must match [`tools/release-signing-key.pub`](tools/release-signing-key.pub).

Advance `workspace.package.version` in `Cargo.toml` before each release. The autotag workflow never increments patch versions.

## Contributing

- Read [CONTRIBUTING.md](CONTRIBUTING.md) before you submit a pull request.
- Read [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) for community rules.
- Use [SUPPORT.md](SUPPORT.md) for support and issue reporting.

## License

[MIT](LICENSE)
