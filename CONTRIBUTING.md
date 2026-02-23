# Contributing

Oore CI is in alpha. Contributions are welcome, but expect things to move fast and break.

## Getting Started

1. Fork and clone the repo
2. Install prerequisites: Rust (stable), Bun
3. Run `bun install` to set up JS dependencies
4. Run `make check` to verify everything compiles

## Development Workflow

- Create a branch from `master`
- Make your changes
- Run `make check` to lint and type-check
- Run `make test-web` and `make test-rust` for tests
- Open a PR against `master`

## Architecture Notes

The project has two main stacks:

- **Frontend:** React 19, TanStack Router (file-based routing), TanStack Query, Tailwind v4, shadcn (Base UI primitives)
- **Backend:** Rust, Axum 0.8, Tokio, SQLite (via sqlx), OIDC auth

Read `docs/README.md` for internal docs pointers and the Linear docs index:

- https://linear.app/oorebuild/document/docs-index-linear-first-457d9edc9cda

## Feature Documentation

Internal technical docs are Linear-first (see `docs/README.md`). Every user-facing feature needs a Linear feature doc using the template:

- https://linear.app/oorebuild/document/feature-doc-template-9f1845da4b46

Any code change under `apps/`, `crates/`, `tools/`, etc. must add an entry to `docs/changes.md`. CI runs `make docs-check` to enforce the change-ledger rule.

## What We're Looking For

- Bug fixes
- Test coverage improvements
- Documentation improvements
- Runner support for additional platforms

## Reporting Issues

Open an issue on GitHub. Include:
- What you expected vs what happened
- Steps to reproduce
- OS and toolchain versions (`rustc --version`, `bun --version`)

For help channels and non-bug paths, see [SUPPORT.md](SUPPORT.md).
Please also follow [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
