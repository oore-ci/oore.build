# Change ledger

## 2026-07-19

- **Rust runtime optimization and hardening**:
  - Runner artifacts are hashed and uploaded from files in bounded chunks; local daemon uploads claim their single-use token before reading the body and publish through an atomic temporary-file rename.
  - Ordinary daemon SQL traffic uses the shared SQLite pool without taking the setup-transition mutex, and queued build claims use a covering queue index.
  - Live build logs use bounded scheduler wakeups with a 15-second recovery query while preserving one-second authorization revalidation.
  - CLI backup and update checksums are chunked, updater archives are verified from temporary files, and authenticated status resources start concurrently.
  - SCM integrations reuse one bounded no-redirect client, effective UID checks avoid subprocesses, dependency features are narrowed, and release and validation Cargo commands use the lockfile.
  - Omitting `limit` from `GET /v1/projects/{project_id}/artifacts` now returns at most 200 artifacts.
