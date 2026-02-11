---
status: implemented
---

# Embedded Runner

By default, `oored` starts an embedded build runner in the same process. This is the simplest deployment — no additional configuration needed.

## How it works

When `oored run` starts without `OORED_RUNNER_MODE=external`, the daemon:

1. Starts an embedded runner alongside the API server
2. The runner automatically claims and executes queued builds
3. Build logs stream directly within the same process
4. Artifacts are handled locally

## When to use

The embedded runner is appropriate for:

- **Single-host deployments** — one Mac running both the daemon and builds
- **Development and testing** — getting started quickly
- **Small teams** — low build volume that one machine can handle

## Verify the runner is active

1. Go to **Settings > Runners** in the web UI
2. An embedded runner should appear as `online`
3. Trigger a test build — it should move from `queued` to `running` within seconds

## Switching to external mode

To disable the embedded runner and require external runners:

```bash
export OORED_RUNNER_MODE=external
oored run
```

See [External Runner](/guides/runners/external-runner) for setup instructions.
