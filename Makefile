.PHONY: dev-web dev-docs build-web build-demo deploy-demo deploy-docs build-docs build check \
       test-web lint-web fix-web \
       test-docs lint-docs fix-docs \
       cargo-check run-daemon run-daemon-debug run-daemon-release \
       run-runner register-runner run-cli doctor \
       docs-check ui-init install-local validate

RUNNER_DAEMON_URL ?= http://127.0.0.1:8787
RUNNER_CONFIG ?= $(HOME)/.oore/runner.json
RUNNER_SESSION_TOKEN ?=
RUNNER_NAME ?= $(shell hostname)
OORED_LOG_LEVEL ?= info

# ── Frontend: Web App ─────────────────────────────────────────────
dev-web:
	bun run dev:web

build-web:
	bun run build:web

build-demo:
	cd apps/web && VITE_DEMO_MODE=true bun run build

deploy-demo: build-demo
	wrangler pages deploy apps/web/dist --project-name=oore-demo

test-web:
	cd apps/web && bun run test

lint-web:
	cd apps/web && bun run lint

fix-web:
	cd apps/web && bun run check

# ── Frontend: Docs Site (VitePress) ───────────────────────────────
dev-docs:
	bun run dev:docs

build-docs:
	bun run build:docs

deploy-docs: build-docs
	wrangler pages deploy apps/docs-site/docs/.vitepress/dist --project-name=oore-docs

test-docs:
	cd apps/docs-site && bun run test

lint-docs:
	cd apps/docs-site && bun run lint

fix-docs:
	cd apps/docs-site && bun run check

# ── Backend (Rust) ────────────────────────────────────────────────
cargo-check:
	cargo check --workspace

run-daemon:
	RUST_LOG=$(OORED_LOG_LEVEL) cargo run -p oored -- run --listen 127.0.0.1:8787

run-daemon-debug:
	RUST_LOG=debug cargo run -p oored -- run --listen 127.0.0.1:8787

run-daemon-release:
	RUST_LOG=info cargo run -p oored --release -- run --listen 127.0.0.1:8787

run-runner:
	cargo run -p oore -- runner start --daemon-url $(RUNNER_DAEMON_URL) --config $(RUNNER_CONFIG)

register-runner:
	@test -n "$(RUNNER_SESSION_TOKEN)" || (echo "RUNNER_SESSION_TOKEN is required"; exit 1)
	cargo run -p oore -- runner register --daemon-url $(RUNNER_DAEMON_URL) --token $(RUNNER_SESSION_TOKEN) --name "$(RUNNER_NAME)"

run-cli:
	cargo run -p oore -- setup open --ttl 15m

doctor:
	cargo run -p oore -- doctor

install-local:
	bash scripts/install.sh

test-rust:
	cargo test -p oored --features test-support

# ── Documentation & Validation ────────────────────────────────────
docs-check:
	bun run docs:check

ui-init:
	bun run ui:init

# ── Aggregate Targets ─────────────────────────────────────────────
build: build-web build-docs cargo-check

check: lint-web cargo-check

validate: docs-check build-web build-docs cargo-check
