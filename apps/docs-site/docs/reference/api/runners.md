---
status: implemented
description: "API endpoints for build runner registration and job management in Oore CI."
---

# Runners API

Endpoints for runner registration, heartbeats, job claiming, and job management.

## Register Runner {#register-runner}

Register a new external runner with the daemon.

```
POST /v1/runners/register
```

**Authentication**: User session (Bearer)

### Request body

```json
{
  "name": "mac-mini-builder",
  "capabilities": {
    "platforms": ["android", "ios", "macos"]
  }
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | `string` | No | Display name for the runner |
| `capabilities` | `object` | No | Runner capabilities (platforms, tools) |

### Response `200 OK`

```json
{
  "id": "runner_abc123",
  "name": "mac-mini-builder",
  "token": "runner_token_xyz...",
  "status": "online",
  "created_at": 1738800000
}
```

The `token` is used for all subsequent runner-to-daemon communication.

---

## List Runners {#list-runners}

```
GET /v1/runners
```

**Authentication**: User session (Bearer)

### Response `200 OK`

```json
[
  {
    "id": "runner_abc123",
    "name": "mac-mini-builder",
    "status": "online",
    "capabilities": { ... },
    "last_heartbeat_at": 1738800060,
    "registered_by": "user_def456",
    "created_at": 1738800000,
    "updated_at": 1738800060
  }
]
```

### Runner status values

| Status | Description |
|---|---|
| `online` | Runner is healthy and accepting jobs |
| `offline` | Runner hasn't sent a heartbeat recently |
| `busy` | Runner is currently executing a build |
| `draining` | Runner is finishing current work and won't accept new jobs |

---

## Update Runner {#update-runner}

```
PATCH /v1/runners/{runner_id}
```

**Authentication**: Runner token (Bearer)

---

## Runner Heartbeat {#runner-heartbeat}

Runners send periodic heartbeats to indicate they are alive and ready for work.

```
POST /v1/runners/{runner_id}/heartbeat
```

**Authentication**: Runner token (Bearer)

---

## Claim Job {#claim-job}

Runner claims the next available build job.

```
POST /v1/runners/{runner_id}/claim
```

**Authentication**: Runner token (Bearer)

### Response `200 OK`

Returns the claimed job details:

```json
{
  "build_id": "build_xyz789",
  "project_id": "proj_def456",
  "pipeline_id": "pipe_abc123",
  "build_number": 42,
  "config_snapshot": { ... },
  "commit_sha": "a1b2c3d4e5f6...",
  "branch": "main",
  "lease_expires_at": 1738803600
}
```

If no jobs are available, returns `204 No Content`.

---

## Get Checkout Auth {#get-checkout-auth}

Runner fetches checkout credentials for its assigned job.

```
GET /v1/runners/{runner_id}/jobs/{job_id}/checkout-auth
```

**Authentication**: Runner token (Bearer)

The response contains credentials only when the linked source requires them, such as a private GitLab repository connected with a Personal Access Token or OAuth token. Runners use these credentials for Git HTTPS checkout without writing them to build logs.

---

## Get Job Status {#get-job-status}

```
GET /v1/runners/{runner_id}/jobs/{job_id}
```

**Authentication**: Runner token (Bearer)

---

## Update Job Status {#update-job-status}

Runner reports build progress or completion.

```
POST /v1/runners/{runner_id}/jobs/{job_id}/status
```

**Authentication**: Runner token (Bearer)

---

## Get Job Android Signing {#get-job-android-signing}

Retrieve Android signing configuration for a running job.

```
GET /v1/runners/{runner_id}/jobs/{job_id}/android-signing
```

**Authentication**: Runner token (Bearer)

Returns the keystore file and credentials needed for Android signing.

---

## Get Job iOS Signing {#get-job-ios-signing}

Retrieve iOS signing configuration for a running job.

```
GET /v1/runners/{runner_id}/jobs/{job_id}/ios-signing
```

**Authentication**: Runner token (Bearer)

Returns the certificate, provisioning profile, and related credentials.
