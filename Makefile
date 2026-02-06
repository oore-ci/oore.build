.PHONY: dev-web dev-docs build-web build-docs build check \
       test-web lint-web fix-web \
       cargo-check run-daemon run-cli \
       docs-check ui-init validate

# ── Frontend: Web App ─────────────────────────────────────────────
dev-web:
	bun run dev:web

build-web:
	bun run build:web

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

# ── Backend (Rust) ────────────────────────────────────────────────
cargo-check:
	cargo check --workspace

run-daemon:
	RUST_LOG=debug cargo run -p oored -- run --listen 127.0.0.1:8787

run-cli:
	cargo run -p oore -- setup open --ttl 15m

# ── Documentation & Validation ────────────────────────────────────
docs-check:
	bun run docs:check

ui-init:
	bun run ui:init

# ── Aggregate Targets ─────────────────────────────────────────────
build: build-web build-docs cargo-check

check: lint-web cargo-check

validate: docs-check build-web build-docs cargo-check
