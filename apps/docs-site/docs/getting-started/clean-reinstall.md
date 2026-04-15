---
status: implemented
description: "Clean reinstall guide for alpha users"
---

# Clean Reinstall Guide for Alpha Users

If you need to reset your Oore CI instance after a mis‑configuration or a failed setup, follow these steps. **Warning:** This will permanently delete all data, including projects, builds, users, and signing credentials.

## 1. Stop the daemon
```bash
# If you started the daemon in the foreground
Ctrl+C
# Or stop a background service (macOS launch agent example)
launchctl bootout gui/$(id -u)/build.oore.oored 2>/dev/null || true
```

## 2. Remove local data

Typical paths for alpha users:

```bash
# macOS default
rm -rf ~/Library/Application\ Support/oore/
```

If you have a custom configuration path, you can use `oore doctor` and `jq` to dynamically find your config directory:

```bash
rm -rf "$(dirname $(oore doctor --json | jq -r .configPath))"
```


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
```
You should see an empty instance with no projects.

---

**Links**
- Linked from the [Public Alpha guide](/getting-started/public-alpha) under *Troubleshooting*.
- Also referenced in [Operations → Troubleshooting](/operations/troubleshooting).
