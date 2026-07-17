---
status: implemented
description: 'The HTTP-based protocol between Oore CI runners and the daemon.'
---

# Runner Protocol

How runners communicate with the Oore CI daemon to claim and execute builds.

## Overview

Runners are processes that execute builds on behalf of the daemon. The protocol is HTTP-based: runners poll the daemon API for available work, claim builds, report progress, and upload artifacts. All runner-facing endpoints require Bearer token authentication using the runner token issued during registration.

## Authentication

Every runner request includes an `Authorization: Bearer <runner_token>` header. The runner token is generated during `POST /v1/runners/register` and must be stored securely by the runner operator. If the token is lost, the runner must be re-registered. Non-loopback runner connections require HTTPS; HTTP is accepted only for a literal loopback IP.

## Lifecycle

```
Register → Heartbeat (periodic) → Claim → Execute → Report status → (repeat)
```

### 1. Registration

An admin registers a new runner via the API or `oore runner register`. The daemon returns a `runner_id` and `runner_token`.

### 2. Heartbeat

The runner periodically calls `POST /v1/runners/{runner_id}/heartbeat` to report its status and capabilities.

**Runner statuses:**

| Status     | Meaning                                           |
| ---------- | ------------------------------------------------- |
| `online`   | Ready to accept work                              |
| `busy`     | Currently executing a build                       |
| `draining` | Will not accept new work; finishing current build |
| `offline`  | Not responding to heartbeats                      |

The daemon uses heartbeat data to track which runners are available.

### 3. Job claim

The runner calls `POST /v1/runners/{runner_id}/claim` with `{"protocol_version": 3}` to request work. The daemon rejects incompatible runners before assigning work.

1. Finds the oldest build with `status = queued`
2. Transitions the build: `queued` → `scheduled` → `assigned`
3. Sets the `runner_id` on the build record
4. Returns a `ClaimedJob` containing everything the runner needs:

| Field              | Description                                                     |
| ------------------ | --------------------------------------------------------------- |
| `build_id`         | Unique build identifier                                         |
| `project_id`       | Parent project                                                  |
| `pipeline_id`      | Pipeline that defines the build                                 |
| `build_number`     | Human-readable build number                                     |
| `config_snapshot`  | Full pipeline configuration captured at build creation time     |
| `commit_sha`       | Git commit to build                                             |
| `branch`           | Git branch                                                      |
| `lease_expires_at` | Deadline for the runner to start reporting progress (5 minutes) |
| `signing_token`    | Ephemeral, job-scoped capability held by the runner parent      |

If no builds are queued, the endpoint returns `204 No Content`.

### 4. Build execution

The runner uses the `config_snapshot` to determine what to build. This snapshot contains:

- The config resolution policy (`file_first_ui_fallback`)
- The `.oore.yaml` config path
- UI-configured execution settings (platforms, build args, environment variables, artifact patterns)
- The commit SHA and branch to check out

The runner clones the repository, checks out the commit, and executes build steps according to the resolved configuration.

### 5. Status reporting

During and after execution, the runner calls `POST /v1/runners/{runner_id}/jobs/{job_id}/status` with:

| Field           | Description                                    |
| --------------- | ---------------------------------------------- |
| `status`        | `running`, `succeeded`, or `failed`            |
| `steps`         | Array of step results (name, status, duration) |
| `exit_code`     | Process exit code (for terminal states)        |
| `error_message` | Error details (for `failed` status)            |

The daemon validates the state transition using the [build state machine](/reference/build-states) and updates the build record.

### 6. Cancellation detection

The runner periodically calls `GET /v1/runners/{runner_id}/jobs/{job_id}` to check if the build has been canceled by a user. If the build status is `canceled`, the runner stops execution.

## Artifact upload

After a successful build, the runner uploads artifacts:

1. **Create artifact record**: `POST /v1/runners/{runner_id}/jobs/{job_id}/artifacts` with the artifact name, type (`apk`, `ipa`, `app`, `generic`), and checksum
2. **Receive upload URL**: The daemon returns a presigned upload URL (S3/R2) or a local upload token
3. **Upload the file**: The runner PUTs the file to the upload URL (max 512 MiB)
4. **Finalize or abort**: The runner calls the artifact `complete` endpoint after a successful upload, or `abort` after a failed upload. Pending artifacts are not visible or downloadable.

See [Artifact Access Model](/concepts/artifact-access) for details on how uploads and downloads are secured.

## Signing credential retrieval

Before repository build stages run, the trusted runner parent can fetch signing credentials with both the runner token and the job-scoped signing capability:

- **Android**: `GET /v1/runners/{runner_id}/jobs/{job_id}/android-signing` — returns keystore and signing config
- **iOS**: `GET /v1/runners/{runner_id}/jobs/{job_id}/ios-signing` — returns certificates, provisioning profiles, and signing identity

These endpoints return decrypted credentials only while the job is assigned or running. The runner keeps them out of repository-controlled files, environment variables, and keychains. It builds unsigned outputs first, then invokes fixed runner-owned Android or iOS signing logic. Terminal and requeue transitions atomically revoke the assignment and signing capability.

## Security considerations

- Runner tokens are long-lived secrets — treat them like API keys
- The daemon verifies the active assignment and a separate per-job signing capability before returning signing material
- Repository-controlled stages do not receive signing credentials or the job-scoped capability
- Artifact upload URLs are time-limited (30-minute TTL)
- Signing credentials are only served to the runner assigned to the build
- Build lease expiry (5 minutes) prevents stale claims from blocking the queue
