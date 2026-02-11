---
status: implemented
---

# Troubleshooting

Common problems and solutions for oore.build operators.

## Daemon won't start

### "Address already in use"

Another process is using port 8787:

```bash
lsof -i :8787
```

Either stop the other process or start oored on a different port:

```bash
oored run --listen 127.0.0.1:8788
```

### "Database locked"

Another oored process or an oore CLI command is holding the database lock. Check for running processes:

```bash
ps aux | grep oore
```

Only one oored instance should run per database file.

### Permission errors on database

```bash
ls -la ~/Library/Application\ Support/oore/
```

Ensure the current user has read/write access to the database and encryption key files.

## Builds stuck in "queued"

### Embedded runner not starting

In default mode, oored starts an embedded runner automatically. If builds stay queued:

1. Check daemon logs for runner-related errors
2. Verify the daemon is running in default mode (not `OORED_RUNNER_MODE=external`)

### External runner not connected

If using external mode:

1. Check that `oore runner start` is running
2. Verify the runner config points to the correct daemon URL
3. Check **Settings > Runners** in the UI for runner status

### Runner claiming but builds failing immediately

Check the build logs for errors. Common causes:

- Flutter/FVM not installed on the runner machine
- Incorrect Flutter version requested
- Missing Xcode for iOS builds

Run `make doctor` on the runner machine to verify the toolchain.

## OIDC authentication failures

### "OIDC discovery failed"

The daemon can't reach the OIDC provider:

1. Verify internet access from the daemon host
2. Test discovery manually:
   ```bash
   curl https://your-issuer-url/.well-known/openid-configuration
   ```
3. Check for trailing slashes in the issuer URL
4. Verify DNS resolution

### "user_not_found" after successful OIDC login

The user authenticated with the OIDC provider but doesn't have an account in oore.build:

1. The user needs to be [invited](/guides/users/invite-users) first
2. The invitation email must exactly match the email in the OIDC provider's ID token

### "Redirect URI mismatch"

The redirect URI sent by oore.build doesn't match what's configured in the OIDC provider:

1. Check the provider's allowed redirect URIs
2. Ensure `http` vs `https` matches
3. Check for port number mismatches
4. Check for trailing slashes

## Signing failures

### "Certificate not found" or "Profile not found"

The signing assets may not be configured for the pipeline:

1. Open the pipeline's **Signing** tab in the UI
2. Verify the certificate and profile are uploaded
3. For API mode, try clicking **Sync** to refresh from App Store Connect

### "codesign failed" in iOS builds

1. Verify the `.p12` password is correct
2. Check that the provisioning profile matches the certificate
3. For ad hoc builds, verify the test device is registered in the profile

## Artifact download failures

### "Download link expired"

Generate a new download link — they are time-limited.

### "Storage not configured"

Go to **Settings > Artifact Storage** and configure a storage backend. Without storage configured, artifact uploads silently fail.

## Database issues

### Corrupt database

If the SQLite database is corrupt:

1. Stop the daemon
2. Restore from a backup (see [Backup and Restore](/operations/backup-restore))
3. If no backup exists, delete the database file and re-run setup

### Reset the instance

To completely reset an instance:

```bash
rm ~/Library/Application\ Support/oore/oore.db
rm ~/Library/Application\ Support/oore/encryption.key
oored run
```

::: danger
This permanently destroys all instance data including users, projects, builds, and signing credentials.
:::
