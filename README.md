<p align="center">
  <img src=".github/logo.svg" alt="oore.build" width="80" height="80" />
</p>

<h1 align="center">oore.build</h1>

<p align="center">Self-hosted, Flutter-first mobile CI and internal app distribution platform.</p>

<p align="center">
  <a href="https://docs.oore.build">Documentation</a>
</p>

<p align="center">
  <a href="https://zerodha.tech"><img src="https://zerodha.tech/static/images/github-badge.svg" /></a>
</p>

> **Alpha** — oore.build is under active development. APIs, config formats, and CLI flags will change without notice. Use at your own risk.

## What is this?

oore.build lets you run your own mobile CI server. V1 targets Android, iOS, and macOS Flutter builds on a macOS host. It provides:

- A **daemon** (`oored`) that orchestrates builds and serves the API
- An **operator CLI** (`oore`) for setup, admin, and runner management
- A **web UI** for managing builds, apps, and team access
- **OIDC-only authentication** — no local passwords

## Prerequisites

- macOS (backend requirement for V1)
- `curl`, `tar`, and `shasum` (for release installer)

For source development, also install [Rust](https://rustup.rs/) and [Bun](https://bun.sh/).

## Quick Start

```bash
# Install latest release binaries (macOS)
curl -fsSL https://oore.build/install | bash
```

Then open `https://ci.oore.build`, add your backend instance URL, and complete setup.

Detailed setup docs: `https://docs.oore.build`

## Development (from source)

```bash
bun install

make run-daemon       # Start oored on 127.0.0.1:8787
make run-cli          # Generate setup token
make dev-web          # Local web UI (http://localhost:3000)
```

## Project Structure

```
apps/web/           React 19 + TanStack Router (product UI)
apps/docs-site/     VitePress documentation site
apps/site/          Neutral landing/install site (`oore.build`)
crates/oored/       Daemon — Axum HTTP server
crates/oore/        Operator CLI — Clap
crates/oore-runner/ Build runner agent
crates/oore-contract/ Shared data types (Serde structs)
docs/               Design docs, platform contract, feature specs
```

## Common Commands

All commands are available as `make` targets:

```bash
make dev-web          # Web UI dev server (port 3000)
make dev-docs         # Docs dev server (VitePress)
make dev-site         # Landing site dev server (port 3002)
make build            # Build everything (web + docs + site + cargo check)
make check            # Lint web + cargo check
make test-web         # Run web tests (Vitest)
make test-rust        # Run Rust tests
make cargo-check      # Type-check Rust workspace
make run-daemon       # Start oored on 127.0.0.1:8787
make run-cli          # Open a setup window (15 min TTL)
make doctor           # Check system prerequisites
```

## Release Automation (macOS + R2)

Releases are published from a dedicated Mac mini:

```bash
make release-local TAG=v0.2.0          # Build + upload one release to R2
sudo make install-release-webhook-daemon  # Install LaunchDaemon webhook listener
make install-release-poller               # Optional polling fallback
```

Artifacts are published under `https://dl.oore.build/releases/`.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE)
