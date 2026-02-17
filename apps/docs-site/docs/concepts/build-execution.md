---
status: implemented
description: "How Oore CI executes Flutter builds from queue to artifact storage."
---

# Build Execution

This page explains what happens from the moment a build is triggered to when artifacts are ready for download.

## Trigger to artifact

```
Trigger → Queue → Claim → Clone → Setup → Build → Collect → Store
```

### 1. Trigger

A build can be triggered three ways:

- **Manual** — user clicks "Trigger Build" in the UI or calls the API
- **Webhook** — GitHub or GitLab sends a push or pull request event
- **API** — direct `POST /v1/projects/{project_id}/builds`

The daemon creates a build record with status `queued` and a snapshot of the pipeline configuration at that moment.

### 2. Queue and scheduling

The build enters the queue. The scheduler picks the oldest queued build and assigns it to an available runner. In V1, there is no capability matching — the scheduler simply picks the oldest job.

### 3. Claim

The runner polls for work via `POST /v1/runners/{runner_id}/claim`. When it claims a build, the build transitions to `assigned` and the runner receives:

- The config snapshot (build commands, platforms, environment variables)
- The repository URL and branch/commit to build
- Signing configuration (if configured)

### 4. Clone

The runner clones the repository at the specified branch and commit. For GitHub App integrations, the runner uses an installation access token. For GitLab, it uses the stored OAuth credentials.

### 5. Setup

The runner prepares the build environment:

1. **Flutter version** — Checks for `.fvmrc` in the repo, falls back to `flutter_version` from config
2. **FVM install** — Runs `fvm install` to ensure the correct Flutter version is available
3. **Environment** — Sets environment variables from the pipeline config

### 6. Build

Commands execute in three stages:

1. **pre_build** — typically `flutter pub get`, code generation
2. **build** — the main build commands (e.g., `flutter build apk --release`)
3. **post_build** — optional post-processing

Each command runs in the cloned repository directory. Output is streamed to the daemon in real-time via `POST /v1/runners/{runner_id}/jobs/{job_id}/logs`.

If any command returns a non-zero exit code, the build fails immediately and subsequent commands are skipped.

### 7. Collect artifacts

After a successful build, the runner scans the workspace for files matching the artifact patterns (e.g., `**/*.apk`, `**/*.ipa`). Matched files are uploaded to the daemon via `POST /v1/runners/{runner_id}/jobs/{job_id}/artifacts`.

### 8. Store

The daemon stores artifacts according to the instance's storage configuration:

| Storage | How it works |
|---|---|
| **Local** | Files stored on the daemon's filesystem. Downloaded via signed token URLs. |
| **S3** | Files uploaded to an S3 bucket. Downloaded via pre-signed S3 URLs. |
| **R2** | Files uploaded to Cloudflare R2. Downloaded via pre-signed R2 URLs. |

Download links are time-limited and generated on demand via `POST /v1/artifacts/{artifact_id}/download-link`.

## Embedded vs. external runners

### Embedded runner (default)

When `oored` starts in default mode, it runs an embedded runner in the same process. This is the simplest setup — no additional configuration needed. The embedded runner:

- Starts automatically with the daemon
- Claims and executes builds locally
- Has access to the local filesystem for artifact storage
- Suitable for single-host deployments

### External runner

For more control, set `OORED_RUNNER_MODE=external` and start a separate runner process:

```bash
oore runner register --daemon-url http://127.0.0.1:8787 --token <session_token>
oore runner start
```

External runners are useful for:

- Running builds on a different machine than the daemon
- Isolating build environments
- Future multi-runner setups

## Config resolution at build time

When a build starts, the runner resolves configuration:

1. Checks the repository for `.oore.yaml` / `.oore.yml`
2. If found, uses the file config (ignoring the pipeline's UI config)
3. If not found, uses the pipeline's execution config from the database
4. Checks for `.fvmrc` for Flutter version (overrides any config-level setting)
