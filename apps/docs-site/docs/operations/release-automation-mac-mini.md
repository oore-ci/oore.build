---
status: implemented
description: "Automate Oore CI releases on a Mac mini using Woodpecker CI and GitHub Releases."
---

# Release Automation on macOS

Use this flow with a dedicated macOS host (for example, a Mac mini) that runs Woodpecker CI, builds release artifacts, deploys Cloudflare Pages sites, and publishes GitHub Releases.

## Prerequisites

- macOS host with Xcode command line tools
- Rust toolchain installed
- Bun installed (for web asset build + `oore-web` executable compile)
- Woodpecker server + agent running on the macOS host
  - Pin to stable release line (`v3.13.0` as of Feb 23, 2026), not `dev`
  - Use `plugin-git v2.8.1` (or newer stable) for clone behavior parity
- Cloudflare token configured in Woodpecker secrets (for `wrangler pages deploy`)
- GitHub token configured in Woodpecker secrets (for pushing tags and creating releases)

## Workflow

- Merge to `alpha`:
  - CI auto-cuts prerelease tags `vX.Y.Z-alpha.N`.
- Merge to `beta`:
  - CI auto-cuts prerelease tags `vX.Y.Z-beta.N`.
- Merge to `stable`:
  - CI auto-cuts stable tags `vX.Y.Z`.
- PR/push validation:
  - CI installs dependencies and runs `make validate-ci` (full checks split into parallel lanes).
  - CI lints `.woodpecker.yml` with pinned `woodpecker-cli` before running validation lanes.
- Tag push (`v*`):
  - CI builds release artifacts for:
    - `aarch64-apple-darwin`
    - `x86_64-apple-darwin`
  - CI builds the web UI (`apps/web/dist`) and compiles `oore-web` for both macOS architectures.
  - CI deploys Pages sites (site + docs + web in parallel, then demo) using `wrangler pages deploy`.
  - CI creates/updates a GitHub Release and uploads artifacts + checksums + release notes.

## Required Woodpecker Secrets

Set these secrets in Woodpecker (repo/org/global as appropriate):

- `GITHUB_TOKEN`:
  - Used to clone/push and to create GitHub Releases.
  - Must have permission to push to the repo and create releases.
- `CLOUDFLARE_API_TOKEN`:
  - Used by `wrangler pages deploy`.

## Notes

The legacy webhook/poller/R2-based release automation is replaced by Woodpecker pipelines and GitHub Releases.

Before promoting to `stable`, run:

```bash
make validate
make release-smoke
```

Note: post-deploy Pages verification is currently disabled in tag pipelines because Cloudflare deployment list metadata has not been deterministic enough for a safe hard gate.
