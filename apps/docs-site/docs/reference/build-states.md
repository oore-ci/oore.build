---
status: implemented
description: 'Build lifecycle states and transitions in Oore CI.'
---

# Build States

Every build in Oore CI transitions through a defined set of states from creation to completion.

## States

| State       | Description                                              |
| ----------- | -------------------------------------------------------- |
| `queued`    | Build created, waiting for a runner to claim it          |
| `scheduled` | Assigned to a runner's queue, waiting to start execution |
| `assigned`  | Runner has claimed the job via heartbeat/claim cycle     |
| `running`   | Build commands are executing on the runner               |
| `succeeded` | All build commands completed with exit code 0            |
| `failed`    | A build command returned a non-zero exit code            |
| `canceled`  | Build was canceled by a user                             |
| `timed_out` | Build exceeded the maximum execution time                |
| `expired`   | Build sat in the queue too long without being claimed    |

## Transitions

```
queued → scheduled → assigned → running → succeeded
                                       → failed
                                       → timed_out
queued → expired
queued → canceled
running → canceled
```

### Normal flow

1. **queued** — A build is created (manual, webhook, or API trigger)
2. **scheduled** — The scheduler assigns the build to a runner
3. **assigned** — The runner claims the job via `POST /v1/runners/{runner_id}/claim`
4. **running** — The runner starts executing build commands and streaming logs
5. **succeeded** or **failed** — Build commands finish

### Terminal states

Once a build reaches any of these states, it cannot transition further:

- `succeeded`
- `failed`
- `canceled`
- `timed_out`
- `expired`

### Cancelation

A build can be canceled from `queued` or `running` state:

```bash
curl -X POST http://127.0.0.1:8787/v1/builds/{build_id}/cancel \
  -H "Authorization: Bearer <session_token>"
```

Canceling an already-terminal build has no effect.

## Build timing fields

| Field         | Set when                                          |
| ------------- | ------------------------------------------------- |
| `queued_at`   | Build created                                     |
| `started_at`  | Runner begins execution (transition to `running`) |
| `finished_at` | Build reaches a terminal state                    |

## Runner interaction

The runner lifecycle for a build:

1. Runner sends heartbeat: `POST /v1/runners/{runner_id}/heartbeat`
2. Runner claims a job: `POST /v1/runners/{runner_id}/claim`
3. Runner streams logs: `POST /v1/runners/{runner_id}/jobs/{job_id}/logs`
4. Runner reports completion: `POST /v1/runners/{runner_id}/jobs/{job_id}/status`
