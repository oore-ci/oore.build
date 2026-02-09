# Runner Management

oore.build includes a runner management screen for operators under **Settings -> Runners**.

## What you can do

- View all registered runners and their current status (`online`, `busy`, `draining`, `offline`)
- Check heartbeat freshness and capability metadata
- Rename externally registered runners

## Embedded runner behavior

On single-host default setups, `oored` starts an embedded local runner. Embedded runners are visible in the table, but renaming is intentionally blocked because the daemon manages that identity.

If you need custom naming and lifecycle control, use external runners and register them with:

```bash
oore runner register --daemon-url http://127.0.0.1:8787 --token <SESSION_TOKEN> --name "<RUNNER_NAME>"
```

## API support

- `GET /v1/runners` lists runners.
- `PATCH /v1/runners/{runner_id}` renames an external runner.

Rename requires owner/admin permissions and writes an audit log entry (`runner_renamed`).
