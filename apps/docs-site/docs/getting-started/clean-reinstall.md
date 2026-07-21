---
status: implemented
description: 'Clean reinstall guide for alpha users'
---

# Clean Reinstall Guide for Alpha Users

If you need to reset your Oore CI instance after a mis‑configuration or a failed setup, follow these steps. **Warning:** This will permanently delete all data, including projects, builds, users, and signing credentials.

## 1. Stop the backend services

```bash
# If you started the daemon in the foreground
Ctrl+C
# Or remove the managed macOS services
oore runner uninstall-service 2>/dev/null || true
sudo oored uninstall-service --system 2>/dev/null || true
```

## 2. Remove local data

Typical paths for alpha users:

```bash
# macOS default
rm -rf ~/Library/Application\ Support/oore/
```

If the daemon was installed with a custom `--state-file` or data directory,
remove the exact path you configured. `oore doctor` reports toolchain readiness;
it does not discover or delete daemon data.

## 3. Re‑install (optional)

If you want a fresh binary, reinstall:

```bash
curl -fsSL https://oore.build/install | bash
```

## 4. Re‑run the setup wizard

```bash
oore setup
```

## 5. Verify clean state

```bash
oore doctor --json
oore status
```

`oore doctor` should report the runner toolchain, and `oore status` should show
the newly configured instance. Confirm the managed runner is online under
**Settings > Runners** before triggering a build.

---

**Links**

- Linked from the [Public Alpha guide](/getting-started/public-alpha) under _Troubleshooting_.
- Also referenced in [Operations → Troubleshooting](/operations/troubleshooting).
