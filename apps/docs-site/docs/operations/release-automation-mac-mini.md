---
status: implemented
description: "Automate oore.build releases on a Mac mini using Woodpecker CI and GitHub Releases."
---

# Release Automation on macOS

Use this flow with a dedicated macOS host (for example, a Mac mini) that runs Woodpecker CI, builds release artifacts, deploys Cloudflare Pages sites, and publishes GitHub Releases.

## Prerequisites

- macOS host with Xcode command line tools
- Rust toolchain installed
- Bun installed (for web asset build + `oore-web` executable compile)
- Woodpecker server + agent running on the macOS host
- Cloudflare token configured in Woodpecker secrets (for `wrangler pages deploy`)
- GitHub token configured in Woodpecker secrets (for pushing tags and creating releases)

## Workflow

- Merge to `main`:
  - CI bumps `workspace.package.version` (patch increment), commits, and creates a semver tag (for example `v0.2.1`).
  - The bump commit includes `[CI SKIP]` to avoid re-trigger loops.
- Tag push:
  - CI builds release artifacts for:
    - `aarch64-apple-darwin`
    - `x86_64-apple-darwin`
  - CI builds the web UI (`apps/web/dist`) and compiles `oore-web` for both macOS architectures.
  - CI deploys Pages sites (web + docs + site) using `wrangler pages deploy`.
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
