---
status: implemented
description: 'Automate Oore CI releases using GitHub Actions with hosted or self-hosted runners.'
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
  - Serial Cloudflare Pages deployment for the site, docs, web app, and demo
  - Final web build, packaging, release notes, and GitHub Release publication
  - Static release-index publication after all GitHub Release assets are available

## Release index

Update checks do not call the GitHub Releases API from installed Oore instances. After a GitHub Release and its checksums upload successfully, the release workflow deploys a static index to the `oore-releases` Cloudflare Pages project:

- `https://releases.oore.build/latest/alpha.json`
- `https://releases.oore.build/latest/beta.json`
- `https://releases.oore.build/latest/stable.json`
- `https://releases.oore.build/alpha.json`
- `https://releases.oore.build/beta.json`
- `https://releases.oore.build/stable.json`

Each `latest` endpoint returns one release object. Each channel endpoint returns the complete newest-first history for that channel and is the canonical source for future changelog surfaces. GitHub Releases remain the binary asset host.

The Cloudflare account needs a Pages project named `oore-releases` with production branch `production`, and `releases.oore.build` mapped as its custom domain.

## Required Secrets

Set these in GitHub repo settings (Settings > Secrets and variables > Actions):

- `CLOUDFLARE_API_TOKEN`:
  - Used by `wrangler pages deploy` for product surfaces and the release index.

`GITHUB_TOKEN` is automatic. Autotag uses it to push the tag and explicitly dispatch the Release workflow, so no personal access token is required.

## Before Promoting to Stable

```bash
make validate
```
