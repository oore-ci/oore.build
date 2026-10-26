# Contributing to Oore CI

Thank you for contributing to Oore CI.

## Requirements

Install these tools before you start:

- Rust and Cargo
- Bun
- Go
- ShellCheck

## Setup

Fork the repository, then clone your fork.

```bash
git clone https://github.com/<your-name>/oore.build.git
cd oore.build
bun install
```

Create a branch for one focused change.

```bash
git switch -c fix/short-description
```

Use the root Makefile for common tasks.

```bash
make dev-web
make validate-frontend
make validate-rust
make validate
```

Run the smallest relevant check during development. Run `make validate` before
you open a pull request.

For interface changes, inspect the result at desktop and compact widths. Check
keyboard navigation, focus order, loading states, empty states, and errors.

## Pull requests

Keep each pull request focused. Explain the problem, the change, and the manual
checks. Link the related issue when one exists.

Use conventional commit messages, such as:

```text
fix(auth): reject an expired setup token
feat(runners): show runner capacity
docs: clarify local setup
```

Do not commit credentials, local data, generated build output, or internal work
notes.
