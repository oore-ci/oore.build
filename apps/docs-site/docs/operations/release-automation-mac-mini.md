---
status: implemented
description: "Automate Oore CI releases using GitHub Actions with hosted or self-hosted runners."
---

# Release Automation

CI/CD is driven by GitHub Actions. Workflows use GitHub-hosted runners by default and can be routed to self-hosted runners through the `RUNNER_LINUX` and `RUNNER_MACOS` repository variables.

## Runner Setup

GitHub-hosted runners require no setup. A self-hosted macOS runner must provide `rustup`, Bun, and GitHub CLI, then its label can be assigned to `RUNNER_MACOS`.

## Workflow

- PR/push validation (`validate.yml`):
  - Change-aware Frontend, Docs, and Rust jobs
- Merge to `alpha` / `beta` / `stable` (`autotag.yml`):
  - CI auto-cuts the appropriate semver tag
- Tag push `v*` (`release.yml`):
  - Parallel macOS arm64 and x86_64 Rust builds with target-specific Cargo caches
  - Parallel Cloudflare Pages deployment
  - Final web build, packaging, release notes, and GitHub Release publication

## Required Secrets

Set these in GitHub repo settings (Settings > Secrets and variables > Actions):

- `CLOUDFLARE_API_TOKEN`:
  - Used by `wrangler pages deploy`.

`GITHUB_TOKEN` is automatic. Autotag uses it to push the tag and explicitly dispatch the Release workflow, so no personal access token is required.

## Before Promoting to Stable

```bash
make validate
make release-smoke
```
