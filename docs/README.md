# Internal Docs (Linear-First)

oore.build internal technical docs (contracts, ADRs, audits, and feature docs) are now **stored in Linear**.

This repository keeps only a small index and change ledger under `docs/` to avoid ballooning the repo and to
keep project tracking + documentation in one place.

## Canonical Docs Home

- Linear project: oore.build Docs
  - https://linear.app/oorebuild/project/oorebuild-258feaed8fee
- Docs Index (start here):
  - https://linear.app/oorebuild/document/docs-index-linear-first-457d9edc9cda

## Repo Requirements

- If you change product behavior (apps/, crates/, tools/, etc.), update `docs/changes.md` in the same PR.
- Public documentation stays in-repo under `apps/docs-site/docs/`.
