---
status: implemented
description: "API endpoints for build log retrieval and streaming in oore.build."
---

# Build Logs API

Endpoints for retrieving and streaming build logs.

## Get Build Logs {#get-build-logs}

Retrieve the complete logs for a finished or running build.

```
GET /v1/builds/{build_id}/logs
```

**Authentication**: User session (Bearer)

### Response `200 OK`

Returns the build log content as a JSON array of log entries or as plain text, depending on the `Accept` header.

---

## Stream Build Logs (SSE) {#stream-build-logs}

Stream build logs in real-time using Server-Sent Events.

```
GET /v1/builds/{build_id}/logs/stream
```

**Authentication**: Stream token (query parameter)

This endpoint uses SSE (Server-Sent Events) to push log lines as they are produced by the runner. The connection stays open until the build finishes.

### Usage

1. First, create a stream token:

   ```bash
   curl -X POST http://127.0.0.1:8787/v1/builds/{build_id}/stream-token \
     -H "Authorization: Bearer <session_token>"
   ```

2. Then connect to the SSE stream with the token:

   ```bash
   curl -N "http://127.0.0.1:8787/v1/builds/{build_id}/logs/stream?token=<stream_token>"
   ```

The stream token is short-lived and scoped to the specific build.

---

## Create Stream Token {#create-stream-token}

Generate a short-lived token for connecting to the log stream.

```
POST /v1/builds/{build_id}/stream-token
```

**Authentication**: User session (Bearer)

### Response `200 OK`

```json
{
  "token": "stream_token_abc123...",
  "expires_at": 1738800600
}
```

---

## Append Build Logs (Runner) {#append-build-logs}

Used by runners to append log lines during build execution.

```
POST /v1/runners/{runner_id}/jobs/{job_id}/logs
```

**Authentication**: Runner token (Bearer)

### Request body

Log content to append. Sent as the build executes.

This endpoint is called by the runner process, not by end users.
