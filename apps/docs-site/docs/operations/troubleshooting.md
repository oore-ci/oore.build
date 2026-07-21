---
status: implemented
description: 'Diagnose and fix common Oore CI issues including builds, auth, and connectivity.'
---

# Troubleshooting

Common problems and solutions for Oore CI operators.

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

### `encryption.key` permission errors

`oored` uses file-based key storage in this release.
If startup fails due encryption key file permissions, fix ownership on the data directory:

```bash
chown -R "$USER":staff ~/Library/Application\ Support/oore
chmod 700 ~/Library/Application\ Support/oore
chmod 600 ~/Library/Application\ Support/oore/encryption.key 2>/dev/null || true
```

## Builds stuck in "queued"

### Scheduling is paused or the source is unavailable

The queued build page reports one of two reasons:

- **Instance paused** — an Owner or Admin must turn on **Accept new builds** in **Settings > Preferences**.
- **Repository unavailable** — reconnect or rediscover the source repository before retrying.

Blocked jobs stay queued, but do not prevent the runner from claiming another eligible job.

### Direct macOS runner not connected

1. Check that the managed runner service is running, or use `oore runner start` for a foreground diagnostic.
2. Verify the runner config points to the correct daemon URL.
3. Check **Settings > Runners** in the UI for runner status.

For a backend-host runner, repair enrollment and the boot-time service by running
`oore runner install-service --managed-local` as the runner account. The command requests
administrator access for launchd setup; do not run the whole command with
`sudo`.

### Runner claiming but builds failing immediately

Check the build logs for errors. Common causes:

- Flutter/FVM not installed on the runner machine
- Incorrect Flutter version requested
- Missing Xcode for iOS builds

Run `oore doctor --all` on the runner machine to verify the toolchain.

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

The user authenticated with the OIDC provider but doesn't have an account in Oore CI:

1. The user needs to be [invited](/guides/users/invite-users) first
2. The invitation email must exactly match the email in the OIDC provider's ID token

### "Redirect URI mismatch"

The redirect URI sent by Oore CI doesn't match what's configured in the OIDC provider:

1. Check the provider's allowed redirect URIs
2. Ensure `http` vs `https` matches
3. Check for port number mismatches
4. Check for trailing slashes

## Hosted UI connectivity issues

### Hosted UI shows "Failed to fetch" during setup/login

If you are using `https://ci.oore.build`, your backend must be reachable over HTTPS.
The hosted UI cannot access local-only HTTP addresses like `http://127.0.0.1:8787`.

Use one of these paths:

1. CLI-only setup:
   ```bash
   oore setup
   ```
2. Expose backend via tunnel:
   ```bash
   cloudflared tunnel --url http://127.0.0.1:8787
   ```
3. Run bundled local web UI for local-only backend development:
   ```bash
   oore-web --backend-url http://127.0.0.1:8787
   ```
   Then add an instance and leave **Backend URL** empty in the UI.

### Local web UI does not open

Check local web logs:

```bash
cat ~/.oore/logs/oore-web.log
```

Verify local web health:

```bash
curl http://127.0.0.1:4173/__oore_web_healthz
```

If launch-at-login was enabled and you need to reload it:

```bash
launchctl bootout gui/$(id -u)/build.oore.oore-web 2>/dev/null || true
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/build.oore.oore-web.plist
```

### Daemon service does not start

Check the launchd service and daemon logs:

```bash
sudo launchctl print system/build.oore.oored
tail -n 200 ~/.oore/logs/oored.log
```

Repair the managed backend services:

```bash
sudo oored install-service --system --user "$USER" --listen 127.0.0.1:8787
oore runner install-service --managed-local --daemon-url http://127.0.0.1:8787
```

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
