.PHONY: dev-web dev-docs dev-site build-web bundle-check build-demo deploy-demo deploy-web build-site deploy-site build-docs deploy-docs build-release-index deploy-release-index-only test-release-index web-performance-baseline test-web-performance-baseline test-web-runtime-performance build check \
		       test-web test-web-ui test-demo lint-web fix-web lint-site fix-site \
		       test-direct-runner-upgrade-smoke \
		       test-docs lint-docs fix-docs test-rust test-install \
		       format-oxc format-oxc-check fmt-rust fmt-rust-check clippy-rust test-rust-workspace lint test \
		       cargo-check run-daemon run-daemon-debug run-daemon-release \
		       run-runner register-runner run-cli doctor clean-dev-state dev-fresh-setup \
		       install-local validate gen-openapi \
		       direct-runner-upgrade-smoke \
		       portless-proxy portless-alias-api portless-list

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
PAGES_PROJECT_RELEASES ?= oore-releases
PAGES_RELEASES_BRANCH ?= production
PAGES_BRANCH ?=
PAGES_COMMIT_HASH ?=
PAGES_COMMIT_MESSAGE ?=
RELEASE_INDEX_SOURCE ?= dist/github-releases.json
RELEASE_INDEX_OUTPUT ?= dist/release-index
RELEASE_INDEX_REPOSITORY ?= oore-ci/oore.build

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

bundle-check: build-web
	bun run bundle:check

deploy-web: build-web
	$(WRANGLER) pages deploy apps/web/dist --project-name=$(PAGES_PROJECT_WEB)$(PAGES_BRANCH_FLAG)$(PAGES_COMMIT_HASH_FLAG)$(PAGES_COMMIT_MESSAGE_FLAG) --commit-dirty=true

deploy-web-only:
	$(WRANGLER) pages deploy apps/web/dist --project-name=$(PAGES_PROJECT_WEB)$(PAGES_BRANCH_FLAG)$(PAGES_COMMIT_HASH_FLAG)$(PAGES_COMMIT_MESSAGE_FLAG) --commit-dirty=true

run-demo:
	cd apps/web && VITE_DEMO_MODE=true bun run dev

build-demo:
	cd apps/web && VITE_DEMO_MODE=true bun run build

deploy-demo: build-demo
	$(WRANGLER) pages deploy apps/web/dist --project-name=$(PAGES_PROJECT_DEMO)$(PAGES_BRANCH_FLAG)$(PAGES_COMMIT_HASH_FLAG)$(PAGES_COMMIT_MESSAGE_FLAG) --commit-dirty=true

deploy-demo-only:
	$(WRANGLER) pages deploy apps/web/dist --project-name=$(PAGES_PROJECT_DEMO)$(PAGES_BRANCH_FLAG)$(PAGES_COMMIT_HASH_FLAG)$(PAGES_COMMIT_MESSAGE_FLAG) --commit-dirty=true

test-web:
	cd apps/web && bun run test

test-web-ui:
	cd apps/web && bun run test:ui

test-demo:
	cd apps/web && bun run test src/demo/demo.test.ts src/hooks/use-permissions.test.ts

lint-web:
	cd apps/web && bun run lint

fix-web:
	cd apps/web && bun run fix

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

# ── Release discovery index ───────────────────────────────────────
build-release-index:
	bun tools/generate-release-index.ts $(RELEASE_INDEX_SOURCE) $(RELEASE_INDEX_OUTPUT) $(RELEASE_INDEX_REPOSITORY)

deploy-release-index-only:
	$(WRANGLER) pages deploy $(RELEASE_INDEX_OUTPUT) --project-name=$(PAGES_PROJECT_RELEASES) --branch=$(PAGES_RELEASES_BRANCH)$(PAGES_COMMIT_HASH_FLAG)$(PAGES_COMMIT_MESSAGE_FLAG) --commit-dirty=true

test-release-index:
	bun test tools/generate-release-index.test.ts

test-direct-runner-upgrade-smoke:
	bun test tools/direct-runner-upgrade-smoke.test.ts

web-performance-baseline:
	bun tools/web-performance-baseline.ts

test-web-performance-baseline:
	bun test tools/web-performance-baseline.test.ts

test-web-runtime-performance:
	bun tools/web-runtime-performance.ts

test-docs:
	cd apps/docs-site && bun run test

lint-docs:
	cd apps/docs-site && bun run lint

fix-docs:
	cd apps/docs-site && bun run fix

lint-site:
	cd apps/site && bun run lint

fix-site:
	cd apps/site && bun run fix

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

test-install:
	bash scripts/install-acceptance.sh

# ── Rust: Lint/Fmt/Clippy/Test ───────────────────────────────────
fmt-rust:
	cargo fmt

fmt-rust-check:
	cargo fmt --check

clippy-rust:
	cargo clippy --workspace --all-targets --all-features -- -D warnings

test-rust-workspace:
	cargo test --workspace

# Release automation lives in GitHub Actions (tag -> GitHub release).
# ── OpenAPI Spec Generation ───────────────────────────────────────
gen-openapi:
	cargo run -p oored --bin openapi-export > apps/docs-site/docs/public/openapi.json
	@echo "OpenAPI spec generated → apps/docs-site/docs/public/openapi.json"

# ── Portless (named .localhost URLs for dev) ─────────────────────
# Start the portless reverse proxy (run once, stays in background)
portless-proxy:
	portless proxy start

# Alias the oored daemon so it's reachable at api.localhost:1355
portless-alias-api:
	portless alias api.oore $(lastword $(subst :, ,$(OORED_DEV_LISTEN_ADDR)))

# Show all active portless routes
portless-list:
	portless list

# ── Aggregate Targets ─────────────────────────────────────────────
format-oxc:
	bun run format

format-oxc-check:
	bun run format:check

build: build-web build-docs build-site cargo-check

check: format-oxc-check lint-web lint-docs lint-site cargo-check

lint: format-oxc-check lint-web lint-docs lint-site fmt-rust-check

test: test-web test-demo test-docs test-release-index test-direct-runner-upgrade-smoke test-web-performance-baseline test-web-runtime-performance test-rust-workspace

validate: lint test test-web-ui clippy-rust bundle-check build-docs build-site cargo-check

direct-runner-upgrade-smoke:
	@test -n "$$OORE_UPGRADE_SMOKE_SESSION_TOKEN" || (echo "OORE_UPGRADE_SMOKE_SESSION_TOKEN is required"; exit 1)
	@test -n "$$OORE_UPGRADE_SMOKE_EXPECTED_VERSION" || (echo "OORE_UPGRADE_SMOKE_EXPECTED_VERSION is required"; exit 1)
	@bun tools/direct-runner-upgrade-smoke.ts
