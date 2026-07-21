---
status: implemented
description: 'Upgrade Oore CI safely with managed drains, restarts, and rollback.'
---

# Upgrade Procedures

How to upgrade your Oore CI instance to a new version.

## Before upgrading

1. **Read the release notes** for the target version
2. **Create and verify a backup** (see [Backup and Restore](/operations/backup-restore))
3. **Check for breaking changes** in the changelog

## Upgrade steps

### Installed release update

```bash
oore update
```

The updater creates a pre-update data backup and release snapshot, stages the
verified release inside the install root, atomically replaces release files,
reloads managed launchd services, and verifies backend readiness plus the
runner handoff. If migration, readiness, or runner acknowledgement fails, it
restores both the previous release and data. Use `/healthz` for liveness and
`/readyz` for database, migration, and encryption-runtime readiness.

When moving from the older per-repository execution gate, links that were
already approved remain intact. Oore unlinks projects whose source was never
approved and cancels their queued builds rather than silently trusting them.
After the update, an Owner or Admin can restore one from **Project > Settings >
Source repository**.

::: warning First update from a pre-hardening release
Run the current installer instead of the old installed `oore update`:

```bash
export OORE_CHANNEL=stable # use beta or alpha if that is the installed channel
curl -fsSL https://oore.build/install | bash
```

Use the current installer for the same release channel as the existing install.
It verifies and extracts the release outside the install root, then runs its
candidate `oore` updater before changing installed files. The same transaction
drains active work, creates the backup, upgrades the release, migrates older
login-session daemon/runner jobs to boot-time services, verifies the new
processes, and rolls back on failure. After this transition, use `oore update`
normally.
:::

### Source checkout update

```bash
cd /path/to/Oore CI
git fetch origin
git checkout <target-version-tag>
```

### 2. Rebuild

```bash
cargo build --release -p oored
cargo build --release -p oore
bun install
make build-web
```

### 3. Stop the daemon

Stop the running `oored` process.

### 4. Start the new version

```bash
./target/release/oored run
```

The daemon handles any necessary database migrations automatically on startup.

### 5. Verify

```bash
curl http://127.0.0.1:8787/healthz
curl http://127.0.0.1:8787/readyz
curl http://127.0.0.1:8787/v1/public/setup-status
```

Check the daemon logs for any migration messages or errors.

## Rollback

If the upgrade causes issues:

1. Stop the daemon
2. Restore the backup (see [Backup and Restore](/operations/backup-restore))
3. Check out the previous version: `git checkout <previous-version>`
4. Rebuild and restart

::: warning
Database migrations are generally forward-only. Restoring a backup after a migration revert is the safest rollback approach.
:::
