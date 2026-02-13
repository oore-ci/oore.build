---
status: implemented
description: "Monitor oore.build health with Prometheus metrics and alerting."
---

# Monitoring

oore.build exposes a Prometheus-compatible metrics endpoint and structured logging for monitoring.

## Health check

```bash
curl http://127.0.0.1:8787/healthz
# Returns: {"ok": true}
```

Use this endpoint for load balancer health checks and uptime monitoring.

## Prometheus metrics

```bash
curl http://127.0.0.1:8787/metrics
```

Returns Prometheus-format metrics. Configure your Prometheus instance to scrape this endpoint:

```yaml
scrape_configs:
  - job_name: oore
    static_configs:
      - targets: ['127.0.0.1:8787']
    metrics_path: /metrics
    scrape_interval: 15s
```

::: tip
Restrict access to `/metrics` in your reverse proxy to prevent exposing internal metrics publicly.
:::

## Structured logging

The daemon uses Rust's `tracing` framework. Control log verbosity with the `RUST_LOG` environment variable:

```bash
# Default
export RUST_LOG=info

# Debug logging
export RUST_LOG=debug

# Module-specific logging
export RUST_LOG=oored=debug,tower_http=info
```

## What to monitor

| Check | How | Alert threshold |
|---|---|---|
| **Daemon up** | `GET /healthz` returns 200 | Any non-200 response |
| **Setup state** | `GET /v1/public/setup-status` | State unexpectedly not `ready` |
| **Runner health** | Check runner heartbeats in UI | Runner offline for >5 minutes |
| **Build queue depth** | Monitor builds in `queued` state | Builds queued for >10 minutes |
| **Disk space** | System monitoring | <10% free on artifact storage volume |
| **Database size** | `ls -la ~/Library/Application Support/oore/oore.db` | Growing unexpectedly |

## Log aggregation

For centralized logging, redirect daemon output to a log aggregator:

```bash
oored run 2>&1 | tee -a /var/log/oored.log
```

Or use `launchd` with stdout/stderr redirection if running as a service.
