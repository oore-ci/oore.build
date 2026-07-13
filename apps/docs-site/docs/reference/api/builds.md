---
status: implemented
description: "API endpoints for triggering, monitoring, and canceling builds in Oore CI."
---

# Builds API

Endpoints for triggering, querying, and managing builds. All endpoints require a valid user session.

## Create Build {#create-build}

Trigger a new build for a project.

```
POST /v1/projects/{project_id}/builds
```

**Authentication**: User session (Bearer)

### Request body

```json
{
  "pipeline_id": "pipe_abc123",
  "branch": "main",
  "commit_sha": "a1b2c3d4e5f6...",
  "trigger_ref": "PR #42"
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `pipeline_id` | `string` | Yes | ID of the pipeline to build |
| `branch` | `string` | No | Branch to resolve before queueing; defaults to the project's default branch |
| `commit_sha` | `string` | No | Specific commit to build |
| `trigger_ref` | `string` | No | Reference string (e.g., PR number) |

### Response `200 OK`

When `commit_sha` is omitted, Oore resolves `branch` to its current commit before queueing and returns that SHA. Reruns retain the original SHA.

```json
{
  "id": "build_xyz789",
  "project_id": "proj_def456",
  "pipeline_id": "pipe_abc123",
  "build_number": 42,
  "status": "queued",
  "trigger_type": "manual",
  "branch": "main",
  "queued_at": 1738800000,
  "created_at": 1738800000,
  "updated_at": 1738800000
}
```

---

## List Builds {#list-builds}

```
GET /v1/builds
```

**Authentication**: User session (Bearer)

### Response `200 OK`

Returns an array of build objects.

---

## Get Build {#get-build}

```
GET /v1/builds/{build_id}
```

**Authentication**: User session (Bearer)

### Response `200 OK`

Returns the full build object including step results, runner assignment, and timing.

```json
{
  "id": "build_xyz789",
  "project_id": "proj_def456",
  "pipeline_id": "pipe_abc123",
  "build_number": 42,
  "status": "succeeded",
  "trigger_type": "webhook",
  "trigger_actor": "github",
  "trigger_event": "push",
  "trigger_ref": "refs/heads/main",
  "commit_sha": "a1b2c3d4e5f6...",
  "branch": "main",
  "config_snapshot": { ... },
  "runner_id": "runner_abc",
  "step_results": [ ... ],
  "exit_code": 0,
  "queued_at": 1738800000,
  "started_at": 1738800060,
  "finished_at": 1738800360,
  "created_at": 1738800000,
  "updated_at": 1738800360
}
```

### Build status values

| Status | Description |
|---|---|
| `queued` | Waiting for a runner to claim |
| `scheduled` | Assigned to a runner, waiting to start |
| `assigned` | Runner has claimed the job |
| `running` | Build commands executing |
| `succeeded` | Build completed with exit code 0 |
| `failed` | Build commands returned non-zero exit code |
| `canceled` | Build was canceled |
| `timed_out` | Build exceeded time limit |
| `expired` | Build sat in queue too long |

---

## Cancel Build {#cancel-build}

```
POST /v1/builds/{build_id}/cancel
```

**Authentication**: User session (Bearer)

### Response `200 OK`

Returns the updated build object with status `canceled`.

Canceling a build that has already finished (succeeded, failed, etc.) has no effect.
