# Installation

This guide covers everything you need to build and run oore.build from source.

## One-command installer (preview)

For single-host macOS setups, the intended UX is:

```bash
curl -fsSL https://oore.build/install | bash
```

The hosted installer endpoint is being finalized. Until it is published, use the source-based flow in this guide.

## Prerequisites

oore.build requires a macOS host for the backend runtime in V1. The frontend can be developed on any platform, but the daemon and CLI are macOS-only.

### Rust toolchain

Install Rust via [rustup](https://rustup.rs/). oore.build uses Rust edition 2024.

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Verify the installation:

```bash
rustc --version   # Rust 1.85+
cargo --version
```

### Bun

The frontend toolchain uses [Bun](https://bun.sh/) as the package manager and runtime.

```bash
curl -fsSL https://bun.sh/install | bash
```

Verify the installation:

```bash
bun --version   # Bun 1.0+
```

### FVM (Flutter Version Manager)

oore.build uses `fvm` to run Flutter commands with pinned versions from `.fvmrc` (or pipeline fallback settings).

```bash
brew tap leoafarias/fvm
brew install fvm
```

Verify installation:

```bash
fvm --version
```

### SQLite

The backend uses SQLite for persistent state. macOS ships with SQLite pre-installed, but you may want a newer version:

```bash
# Optional: install via Homebrew for a newer version
brew install sqlite
```

## Clone the repository

```bash
git clone https://github.com/devaryakjha/oore.build.git
cd oore.build
```

## Install dependencies

Install frontend dependencies with Bun:

```bash
bun install
```

## Build verification

Run the full build to verify everything is working:

::: code-group

```bash [Full build]
make build
```

```bash [Individual targets]
make build-web     # Frontend production build
make build-docs    # VitePress documentation build
make cargo-check   # Rust workspace compile check
```

:::

The `make build` target runs all three checks. If everything succeeds, your environment is ready.

## Development servers

Start the development servers to verify the full stack:

::: code-group

```bash [Daemon]
make run-daemon
# Starts oored on 127.0.0.1:8787
# Uses RUST_LOG=info by default
# Default mode auto-starts an embedded local runner
```

```bash [Web UI]
make dev-web
# Starts Vite dev server on port 3000
```

```bash [Docs site]
make dev-docs
# Starts VitePress dev server on port 4173
```

:::

## Available make targets

All common commands are available as `make` targets from the repository root:

| Target | Description |
|--------|-------------|
| `make dev-web` | Web app dev server (port 3000) |
| `make dev-docs` | VitePress dev server (port 4173) |
| `make build-web` | Production build (web) |
| `make build-docs` | VitePress production build |
| `make test-web` | Run web app tests (Vitest) |
| `make lint-web` | ESLint |
| `make fix-web` | Prettier + ESLint auto-fix |
| `make cargo-check` | Compile check all Rust crates |
| `make run-daemon` | Run oored on 127.0.0.1:8787 (`RUST_LOG=info` by default) |
| `make run-daemon-debug` | Run oored with verbose logs (`RUST_LOG=debug`) |
| `make run-daemon-release` | Run optimized release binary (`--release`, `RUST_LOG=info`) |
| `make register-runner` | Register an external runner (advanced) |
| `make run-runner` | Start external runner process (advanced) |
| `make run-cli` | Run `oore setup open --ttl 15m` |
| `make doctor` | Check required tooling (git, rust, bun, fvm, flutter, xcodebuild) |
| `make install-local` | Run local installer script scaffold (`scripts/install.sh`) |
| `make docs-check` | Validate feature docs against template |
| `make ui-init` | Re-initialize shadcn from shared preset |
| `make build` | build-web + build-docs + cargo-check |
| `make check` | lint-web + cargo-check |
| `make validate` | Full pre-handoff validation |

## Troubleshooting

### Rust compilation errors

If you see errors about missing crate features or edition 2024:

```bash
rustup update stable
```

Edition 2024 requires Rust 1.85 or later.

### Bun lockfile conflicts

If `bun install` fails with lockfile errors:

```bash
rm bun.lock
bun install
```

### SQLite connection errors

The daemon stores its database at `~/Library/Application Support/oore/oore.db` by default. You can override this with:

```bash
export OORE_SETUP_STATE_FILE=/path/to/custom.db
```

Or pass it directly:

```bash
cargo run -p oored -- run --state-file /path/to/custom.db
```

### Build stuck in queued

By default, `oored` starts an embedded local runner and should claim queued builds automatically. If you explicitly set `OORED_RUNNER_MODE=external`, you must run an external runner process (`make run-runner`) for builds to execute.

## Next steps

Once your environment is set up, follow the [Quick Start](/guide/quick-start) to configure your first instance.
