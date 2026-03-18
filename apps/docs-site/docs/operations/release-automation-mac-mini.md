---
status: implemented
description: "Automate Oore CI releases using GitHub Actions on a self-hosted Mac mini runner."
---

# Release Automation

CI/CD is driven by GitHub Actions running on a self-hosted Mac mini runner (`jarvis`). All workflows — validation, autotagging, and release — run on the same machine with zero billable GitHub Actions minutes.

## Runner Setup

The GitHub Actions runner is installed at `~/actions-runner` on jarvis and runs as a launchd user agent (`actions.runner.devaryakjha-oore.build.jarvis`). It auto-starts on login.

Toolchain installed via Homebrew: `rustup`, `bun`, `gh`.

Runner labels: `self-hosted`, `macOS`, `ARM64`, `jarvis`.

## Workflow

- PR/push validation (`validate.yml`):
  - Two parallel jobs on jarvis: Frontend & Docs (bun), Rust (cargo)
- Merge to `alpha` / `beta` / `stable` (`autotag.yml`):
  - CI auto-cuts the appropriate semver tag
- Tag push `v*` (`release.yml`):
  - Single job on jarvis: build web, cross-compile Rust (arm64 + x86_64), package tarballs, generate release notes, deploy to Cloudflare Pages, create GitHub Release with artifacts

## Required Secrets

Set these in GitHub repo settings (Settings > Secrets and variables > Actions):

- `RELEASE_PAT`:
  - Fine-grained PAT with `contents: write` on this repo.
  - Used by the autotag workflow to push tags that trigger the release workflow.
  - (Tags pushed by the automatic `GITHUB_TOKEN` do not trigger downstream workflows.)
- `CLOUDFLARE_API_TOKEN`:
  - Used by `wrangler pages deploy`.

`GITHUB_TOKEN` is automatic and used for GitHub Releases and general CI operations.

## Before Promoting to Stable

```bash
make validate
make release-smoke
```
