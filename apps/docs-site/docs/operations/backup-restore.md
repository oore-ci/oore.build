---
status: implemented
description: 'Create, verify, and restore Oore CI backups.'
---

# Backup and Restore

`oore backup` packages a consistent SQLite snapshot, the matching encryption key, and a checksum manifest into one owner-readable archive. Both database and key are required to recover encrypted credentials.

## Create and verify

```bash
oore backup create --output /Volumes/backups/oore-$(date +%F).tar.gz
oore backup verify --input /Volumes/backups/oore-$(date +%F).tar.gz
```

The snapshot uses SQLite's online backup operation, so creating it does not require stopping `oored`. The archive and copied key are written with restrictive owner-only permissions. Keep archives on separate encrypted storage.

For a non-default database, use the same state-file path as the daemon:

```bash
oore backup create --state-file "$HOME/Library/Application Support/oore-prod/oore.db" --output /Volumes/backups/oore-prod.tar.gz
```

## Restore

Stop the daemon first. Restore refuses to run while the default managed daemon is reachable, preventing a live process from writing over restored state.

```bash
launchctl bootout gui/$(id -u)/build.oore.oored
oore backup restore --input /Volumes/backups/oore-2026-07-12.tar.gz
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/build.oore.oored.plist
```

The command verifies the manifest, both SHA-256 checksums, key length, and SQLite integrity before replacing files. It swaps database and key atomically and restores the previous pair if replacement fails.

## Storage guidance

- Keep at least one verified copy outside the daemon host.
- Treat the archive as sensitive: it contains the encryption key.
- Test `oore backup verify` after copying an archive and rehearse a restore before relying on it.
