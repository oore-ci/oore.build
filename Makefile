.PHONY: dev-web dev-docs dev-site build-web build-demo deploy-demo deploy-web deploy-ci build-site deploy-site build-docs build check \
	       test-web lint-web fix-web \
	       test-docs lint-docs fix-docs test-rust \
	       cargo-check run-daemon run-daemon-debug run-daemon-release \
	       run-runner register-runner run-cli doctor clean-dev-state dev-fresh-setup \
	       docs-check ui-init install-local validate \
	       release-local release-poll-tags release-webhook-server install-release-poller install-release-webhook install-release-webhook-daemon release-cut

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

# ── Frontend: Web App ─────────────────────────────────────────────
dev-web:
	bun run dev:web

build-web:
	bun run build:web

deploy-web: build-web
	wrangler pages deploy apps/web/dist --project-name=oore-ci

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

dev-site:
	bun run dev:site

build-docs:
	bun run build:docs

build-site:
	bun run build:site

deploy-site: build-site
	wrangler pages deploy apps/site/dist --project-name=oore

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
	OORED_DATA_DIR=$(OORED_DEV_DATA_DIR) OORE_SETUP_STATE_FILE=$(OORE_DEV_SETUP_STATE_FILE) RUST_LOG=$(OORED_LOG_LEVEL) cargo run -p oored -- run --listen $(OORED_DEV_LISTEN_ADDR)

run-daemon-debug:
	OORED_DATA_DIR=$(OORED_DEV_DATA_DIR) OORE_SETUP_STATE_FILE=$(OORE_DEV_SETUP_STATE_FILE) RUST_LOG=debug cargo run -p oored -- run --listen $(OORED_DEV_LISTEN_ADDR)

run-daemon-release:
	OORED_DATA_DIR=$(OORED_DEV_DATA_DIR) OORE_SETUP_STATE_FILE=$(OORE_DEV_SETUP_STATE_FILE) RUST_LOG=info cargo run -p oored --release -- run --listen $(OORED_DEV_LISTEN_ADDR)

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
	OORED_DEV_DATA_DIR=$(OORED_DEV_DATA_DIR) OORED_DEV_LISTEN_ADDR=$(OORED_DEV_LISTEN_ADDR) OORE_DEV_DAEMON_URL=$(OORED_DEV_DAEMON_URL) bash scripts/clean-dev-state.sh

dev-fresh-setup:
	OORED_DEV_DATA_DIR=$(OORED_DEV_DATA_DIR) OORE_DEV_SETUP_STATE_FILE=$(OORE_DEV_SETUP_STATE_FILE) OORED_DEV_LISTEN_ADDR=$(OORED_DEV_LISTEN_ADDR) OORE_DEV_DAEMON_URL=$(OORED_DEV_DAEMON_URL) OORE_DEV_ENABLE_TUNNEL=$(OORE_DEV_ENABLE_TUNNEL) OORE_DEV_SETUP_MODE=$(OORE_DEV_SETUP_MODE) bash scripts/dev-fresh-setup.sh

install-local:
	bash scripts/install.sh

test-rust:
	cargo test -p oored --features test-support

release-local:
	@test -n "$(TAG)" || (echo "TAG is required (example: make release-local TAG=v0.2.0)"; exit 1)
	bash scripts/release-local.sh "$(TAG)"

release-cut:
	@test -n "$(VERSION)" || (echo "VERSION is required (example: make release-cut VERSION=0.2.0)"; exit 1)
	bash scripts/release-cut.sh "$(VERSION)"

release-poll-tags:
	bash scripts/release-poll-tags.sh

release-webhook-server:
	bash scripts/release-webhook-server.sh

install-release-poller:
	bash scripts/install-launchd-release-poller.sh

install-release-webhook:
	bash scripts/install-launchd-release-webhook.sh

install-release-webhook-daemon:
	bash scripts/install-launchd-release-webhook-daemon.sh

# ── Documentation & Validation ────────────────────────────────────
docs-check:
	bun run docs:check

ui-init:
	bun run ui:init

# ── Aggregate Targets ─────────────────────────────────────────────
build: build-web build-docs build-site cargo-check

check: lint-web cargo-check

validate: docs-check build-web build-docs build-site cargo-check
