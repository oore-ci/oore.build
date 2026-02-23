.PHONY: dev-web dev-docs dev-site build-web build-demo deploy-demo deploy-web build-site deploy-site build-docs deploy-docs build check \
		       test-web lint-web fix-web \
		       test-docs lint-docs fix-docs test-rust \
		       fmt-rust fmt-rust-check clippy-rust test-rust-workspace lint test \
		       cargo-check run-daemon run-daemon-debug run-daemon-release \
		       run-runner register-runner run-cli doctor clean-dev-state dev-fresh-setup \
		       docs-check lint-woodpecker ui-init install-local validate validate-ci gen-openapi release-smoke \
		       release-local release-cut

RUNNER_DAEMON_URL ?= http://127.0.0.1:8787
RUNNER_CONFIG ?= $(HOME)/.oore/runner.json
RUNNER_SESSION_TOKEN ?=
RUNNER_NAME ?= $(shell hostname)
OORED_LOG_LEVEL ?= info
OORED_DEV_DATA_DIR ?= $(HOME)/.oore/dev.noindex
OORE_DEV_SETUP_STATE_FILE ?= $(OORED_DEV_DATA_DIR)/oore.db
OORED_DEV_LISTEN_ADDR ?= 127.0.0.1:8787
OORED_DEV_DAEMON_URL ?= http://$(OORED_DEV_LISTEN_ADDR)
OORE_DEV_ENABLE_TUNNEL ?= 1
OORE_DEV_SETUP_MODE ?= token
# Wrangler is Node-backed. Running it via `bunx --bun` has proven flaky on macOS
# (observed silent failures in CI and locally). Use the real `wrangler` binary
# by default and install/pin it in CI.
WRANGLER ?= wrangler
PAGES_PROJECT_WEB ?= oore-ci
PAGES_PROJECT_DEMO ?= oore-demo
PAGES_PROJECT_SITE ?= oore
PAGES_PROJECT_DOCS ?= oore-docs
PAGES_BRANCH ?=
PAGES_COMMIT_HASH ?=
PAGES_COMMIT_MESSAGE ?=

# If PAGES_BRANCH is set (e.g. alpha/beta), deploy to a Pages preview branch.
# Important: avoid leaving behind extra whitespace in the shell command when unset.
# `$(if ...)` preserves the leading space in the "then" clause, while plain `:=` assignments do not.
PAGES_BRANCH_FLAG :=$(if $(strip $(PAGES_BRANCH)), --branch=$(PAGES_BRANCH),)
PAGES_COMMIT_HASH_FLAG :=$(if $(strip $(PAGES_COMMIT_HASH)), --commit-hash=$(PAGES_COMMIT_HASH),)
PAGES_COMMIT_MESSAGE_FLAG :=$(if $(strip $(PAGES_COMMIT_MESSAGE)), --commit-message=$(PAGES_COMMIT_MESSAGE),)

# ── Frontend: Web App ─────────────────────────────────────────────
dev-web:
	bun run dev:web

build-web:
	bun run build:web

deploy-web: build-web
	$(WRANGLER) pages deploy apps/web/dist --project-name=$(PAGES_PROJECT_WEB)$(PAGES_BRANCH_FLAG)$(PAGES_COMMIT_HASH_FLAG)$(PAGES_COMMIT_MESSAGE_FLAG) --commit-dirty=true

deploy-web-only:
	$(WRANGLER) pages deploy apps/web/dist --project-name=$(PAGES_PROJECT_WEB)$(PAGES_BRANCH_FLAG)$(PAGES_COMMIT_HASH_FLAG)$(PAGES_COMMIT_MESSAGE_FLAG) --commit-dirty=true

build-demo:
	cd apps/web && VITE_DEMO_MODE=true bun run build

deploy-demo: build-demo
	$(WRANGLER) pages deploy apps/web/dist --project-name=$(PAGES_PROJECT_DEMO)$(PAGES_BRANCH_FLAG)$(PAGES_COMMIT_HASH_FLAG)$(PAGES_COMMIT_MESSAGE_FLAG) --commit-dirty=true

deploy-demo-only:
	$(WRANGLER) pages deploy apps/web/dist --project-name=$(PAGES_PROJECT_DEMO)$(PAGES_BRANCH_FLAG)$(PAGES_COMMIT_HASH_FLAG)$(PAGES_COMMIT_MESSAGE_FLAG) --commit-dirty=true

test-web:
	cd apps/web && bun run test

lint-web:
	cd apps/web && bun run lint

fix-web:
	cd apps/web && bun run check

# ── Frontend: Docs Site (VitePress) ───────────────────────────────
dev-docs:
	bun run dev:docs

dev-site:
	bun run dev:site

build-docs:
	bun run build:docs

build-site:
	bun run build:site

deploy-site: build-site
	$(WRANGLER) pages deploy apps/site/dist --project-name=$(PAGES_PROJECT_SITE)$(PAGES_BRANCH_FLAG)$(PAGES_COMMIT_HASH_FLAG)$(PAGES_COMMIT_MESSAGE_FLAG) --commit-dirty=true

deploy-site-only:
	$(WRANGLER) pages deploy apps/site/dist --project-name=$(PAGES_PROJECT_SITE)$(PAGES_BRANCH_FLAG)$(PAGES_COMMIT_HASH_FLAG)$(PAGES_COMMIT_MESSAGE_FLAG) --commit-dirty=true

deploy-docs: build-docs
	$(WRANGLER) pages deploy apps/docs-site/docs/.vitepress/dist --project-name=$(PAGES_PROJECT_DOCS)$(PAGES_BRANCH_FLAG)$(PAGES_COMMIT_HASH_FLAG)$(PAGES_COMMIT_MESSAGE_FLAG) --commit-dirty=true

deploy-docs-only:
	$(WRANGLER) pages deploy apps/docs-site/docs/.vitepress/dist --project-name=$(PAGES_PROJECT_DOCS)$(PAGES_BRANCH_FLAG)$(PAGES_COMMIT_HASH_FLAG)$(PAGES_COMMIT_MESSAGE_FLAG) --commit-dirty=true

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
	OORED_DATA_DIR=$(OORED_DEV_DATA_DIR) OORE_SETUP_STATE_FILE=$(OORE_DEV_SETUP_STATE_FILE) RUST_LOG=$(OORED_LOG_LEVEL) cargo run -p oored --bin oored -- run --listen $(OORED_DEV_LISTEN_ADDR)

run-daemon-debug:
	OORED_DATA_DIR=$(OORED_DEV_DATA_DIR) OORE_SETUP_STATE_FILE=$(OORE_DEV_SETUP_STATE_FILE) RUST_LOG=debug cargo run -p oored --bin oored -- run --listen $(OORED_DEV_LISTEN_ADDR)

run-daemon-release:
	OORED_DATA_DIR=$(OORED_DEV_DATA_DIR) OORE_SETUP_STATE_FILE=$(OORE_DEV_SETUP_STATE_FILE) RUST_LOG=info cargo run -p oored --release --bin oored -- run --listen $(OORED_DEV_LISTEN_ADDR)

run-runner:
	cargo run -p oore -- runner start --daemon-url $(RUNNER_DAEMON_URL) --config $(RUNNER_CONFIG)

register-runner:
	@test -n "$(RUNNER_SESSION_TOKEN)" || (echo "RUNNER_SESSION_TOKEN is required"; exit 1)
	cargo run -p oore -- runner register --daemon-url $(RUNNER_DAEMON_URL) --token $(RUNNER_SESSION_TOKEN) --name "$(RUNNER_NAME)"

run-cli:
	OORED_DATA_DIR=$(OORED_DEV_DATA_DIR) OORE_SETUP_STATE_FILE=$(OORE_DEV_SETUP_STATE_FILE) OORE_DAEMON_URL=$(OORED_DEV_DAEMON_URL) cargo run -p oore -- setup --daemon-url $(OORED_DEV_DAEMON_URL) token --ttl 15m

doctor:
	cargo run -p oore -- doctor

clean-dev-state:
	OORED_DEV_DATA_DIR=$(OORED_DEV_DATA_DIR) OORED_DEV_LISTEN_ADDR=$(OORED_DEV_LISTEN_ADDR) OORE_DEV_DAEMON_URL=$(OORED_DEV_DAEMON_URL) bash tools/clean-dev-state.sh

dev-fresh-setup:
	OORED_DEV_DATA_DIR=$(OORED_DEV_DATA_DIR) OORE_DEV_SETUP_STATE_FILE=$(OORE_DEV_SETUP_STATE_FILE) OORED_DEV_LISTEN_ADDR=$(OORED_DEV_LISTEN_ADDR) OORE_DEV_DAEMON_URL=$(OORED_DEV_DAEMON_URL) OORE_DEV_ENABLE_TUNNEL=$(OORE_DEV_ENABLE_TUNNEL) OORE_DEV_SETUP_MODE=$(OORE_DEV_SETUP_MODE) bash tools/dev-fresh-setup.sh

install-local:
	bash scripts/install.sh

test-rust:
	cargo test -p oored --features test-support

# ── Rust: Lint/Fmt/Clippy/Test ───────────────────────────────────
fmt-rust:
	cargo fmt

fmt-rust-check:
	cargo fmt --check

clippy-rust:
	cargo clippy --workspace --all-targets --all-features -- -D warnings

test-rust-workspace:
	cargo test --workspace

# Release automation now lives in Woodpecker (tag -> GitHub release).
release-local:
	@echo "Use Woodpecker tag pipeline to publish releases."
	@echo "If you need a manual release, create/push a semver tag like v0.2.0."
	@exit 1

release-cut:
	@echo "Use Woodpecker push-to-main pipeline to auto-cut tags (or push a tag manually)."
	@exit 1

# ── OpenAPI Spec Generation ───────────────────────────────────────
gen-openapi:
	cargo run -p oored --bin openapi-export > apps/docs-site/docs/public/openapi.json
	@echo "OpenAPI spec generated → apps/docs-site/docs/public/openapi.json"

# ── Documentation & Validation ────────────────────────────────────
docs-check:
	bun run docs:check

lint-woodpecker:
	bash tools/lint-woodpecker.sh .woodpecker.yml

ui-init:
	bun run ui:init

# ── Aggregate Targets ─────────────────────────────────────────────
build: build-web build-docs build-site cargo-check

check: lint-web cargo-check

lint: lint-web lint-docs fmt-rust-check

test: test-web test-docs test-rust-workspace

validate: docs-check lint test clippy-rust build-web build-docs build-site cargo-check

validate-ci:
	bash tools/validate-ci.sh

release-smoke:
	bash tools/release-smoke.sh
