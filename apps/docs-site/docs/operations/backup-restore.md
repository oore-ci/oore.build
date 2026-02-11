---
status: implemented
---

# Backup and Restore

oore.build stores all state in two files: the SQLite database and the encryption key. Regular backups of both are essential.

## What to back up

| File | Default location | Contains |
|---|---|---|
| **Database** | `~/Library/Application Support/oore/oore.db` | All instance state, users, projects, builds, configs |
| **Encryption key** | `~/Library/Application Support/oore/encryption.key` | AES-256-GCM key for encrypted secrets |

::: danger
Both files are required for a successful restore. The database contains encrypted secrets that can only be decrypted with the corresponding encryption key. If you lose the encryption key, encrypted data (OIDC client secrets, signing credentials) is unrecoverable.
:::

## Backup

### Manual backup

```bash
# Stop the daemon first for a consistent snapshot
# Or use SQLite's backup API for online backups

# Copy database
cp ~/Library/Application\ Support/oore/oore.db /backups/oore-$(date +%Y%m%d).db

# Copy encryption key
cp ~/Library/Application\ Support/oore/encryption.key /backups/encryption-$(date +%Y%m%d).key
```

### SQLite online backup

For zero-downtime backups, use SQLite's `.backup` command:

```bash
sqlite3 ~/Library/Application\ Support/oore/oore.db ".backup /backups/oore-$(date +%Y%m%d).db"
```

### Automated backups

Schedule backups with cron or launchd:

```bash
# crontab -e
0 2 * * * sqlite3 ~/Library/Application\ Support/oore/oore.db ".backup /backups/oore-$(date +\%Y\%m\%d).db" && cp ~/Library/Application\ Support/oore/encryption.key /backups/encryption-$(date +\%Y\%m\%d).key
```

## Restore

### 1. Stop the daemon

```bash
# Stop oored process
```

### 2. Restore files

```bash
cp /backups/oore-20260210.db ~/Library/Application\ Support/oore/oore.db
cp /backups/encryption-20260210.key ~/Library/Application\ Support/oore/encryption.key
```

### 3. Start the daemon

```bash
oored run
```

### 4. Verify

```bash
curl http://127.0.0.1:8787/v1/public/setup-status
curl http://127.0.0.1:8787/healthz
```

## Backup storage recommendations

- Store backups on a separate volume or remote storage
- Encrypt backups at rest (the encryption key file is especially sensitive)
- Retain at least 7 daily backups
- Test restores periodically
