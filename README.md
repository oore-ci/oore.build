# oore.build

Self-hosted, Flutter-first mobile CI and internal app distribution platform.

> **Alpha** — oore.build is under active development. APIs, config formats, and CLI flags will change without notice. Use at your own risk.

## What is this?

oore.build lets you run your own mobile CI server. V1 targets Android, iOS, and macOS Flutter builds on a macOS host. It provides:

- A **daemon** (`oored`) that orchestrates builds and serves the API
- An **operator CLI** (`oore`) for setup, admin, and runner management
- A **web UI** for managing builds, apps, and team access
- **OIDC-only authentication** — no local passwords

## Prerequisites

- macOS (backend requirement for V1)
- [Rust](https://rustup.rs/) (stable, edition 2024)
- [Bun](https://bun.sh/) (package manager for the frontend)

## Quick Start

```bash
# Install dependencies
bun install

# Start the daemon
make run-daemon

# In another terminal, open a setup window
make run-cli

# In another terminal, start the web UI
make dev-web
```

Then open `http://localhost:3000` and follow the setup wizard.

## Project Structure

```
apps/web/           React 19 + TanStack Router (product UI)
apps/docs-site/     VitePress documentation site
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
make build            # Build everything (web + docs + cargo check)
make check            # Lint web + cargo check
make test-web         # Run web tests (Vitest)
make test-rust        # Run Rust tests
make cargo-check      # Type-check Rust workspace
make run-daemon       # Start oored on 127.0.0.1:8787
make run-cli          # Open a setup window (15 min TTL)
make doctor           # Check system prerequisites
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE)
