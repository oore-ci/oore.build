---
status: implemented
description: "Upgrade Oore CI to a new version with zero-downtime strategies."
---

# Upgrade Procedures

How to upgrade your Oore CI instance to a new version.

## Before upgrading

1. **Read the release notes** for the target version
2. **Back up your database and encryption key** (see [Backup and Restore](/operations/backup-restore))
3. **Check for breaking changes** in the changelog

## Upgrade steps

### 1. Pull the latest code

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
