# Contributing

oore.build is in alpha. Contributions are welcome, but expect things to move fast and break.

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

Read `docs/platform-contract.md` for the full V1 spec and `docs/strict-guidelines.md` for mandatory rules.

## Feature Documentation

Every user-facing feature needs a doc in `docs/features/` following the template at `docs/templates/feature-doc-template.md`. The CI runs `make docs-check` to enforce this.

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
