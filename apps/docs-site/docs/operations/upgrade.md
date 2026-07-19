---
status: implemented
description: 'Upgrade Oore CI to a new version with zero-downtime strategies.'
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

The updater creates a pre-update backup, stages the verified release inside the install root, atomically replaces release files, reloads active launchd daemon/local-web services, and rolls back the release if readiness fails. Use `/healthz` for liveness and `/readyz` for database, migration, and encryption-runtime readiness.

::: warning
This hardening release must be installed with the current installer. An already-installed older updater cannot be made retroactively atomic or rollback-safe.
:::

### Source checkout update

```bash
cd /path/to/Oore CI
git fetch origin
git checkout <target-version-tag>
```

### 2. Rebuild

```bash
cargo build --release -p oored --locked
cargo build --release -p oore --locked
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
