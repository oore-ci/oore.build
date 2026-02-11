---
status: implemented
---

# Trigger Builds

oore.build supports three ways to trigger builds: manual triggers from the UI, webhook triggers from GitHub/GitLab, and direct API calls.

## What you need

- **Role**: any authenticated user (for manual/API triggers)
- A [project](/guides/projects/create-project) with at least one [pipeline](/guides/projects/pipeline-config)
- An active runner (embedded runner runs by default)

## Manual trigger (UI)

1. Open the project in the web UI
2. Click **Trigger Build**
3. Select the pipeline, branch, and optionally a specific commit
4. Click **Start Build**

The build enters `queued` state and is picked up by the next available runner.

## Webhook trigger

Webhooks trigger builds automatically when you push code or open pull requests.

### GitHub

Webhooks are configured automatically when you [connect a GitHub App](/guides/integrations/github-app). Builds are triggered based on the pipeline's trigger configuration (events and branch filters).

### GitLab

Configure a webhook in your [GitLab project settings](/guides/integrations/gitlab). Webhook events are received at:

```
POST /v1/webhooks/gitlab
```

### Trigger filtering

A webhook event triggers a build only if:

1. The event type matches the pipeline's `triggers.events` (e.g., `push`, `pull_request`)
2. The branch matches the pipeline's `triggers.branches` (supports glob patterns)
3. The pipeline is enabled

## API trigger

```bash
curl -X POST http://127.0.0.1:8787/v1/projects/{project_id}/builds \
  -H "Authorization: Bearer <session_token>" \
  -H "Content-Type: application/json" \
  -d '{
    "pipeline_id": "<pipeline_id>",
    "branch": "main",
    "commit_sha": "<optional_commit_sha>"
  }'
```

### Request body

| Field | Type | Required | Description |
|---|---|---|---|
| `pipeline_id` | `string` | Yes | ID of the pipeline to build |
| `branch` | `string` | No | Branch to build (defaults to repository default) |
| `commit_sha` | `string` | No | Specific commit to build |
| `trigger_ref` | `string` | No | Reference string (e.g., PR number) |

### Response `200 OK`

Returns the created build object with its ID and initial `queued` status.

## Monitoring builds

After triggering:

1. The build appears in the project's **Builds** list
2. Click on a build to view real-time logs (streamed via SSE)
3. Build states progress: `queued` → `assigned` → `running` → `succeeded`/`failed`

### List builds via API

```bash
# All builds
curl http://127.0.0.1:8787/v1/builds \
  -H "Authorization: Bearer <session_token>"

# Specific build
curl http://127.0.0.1:8787/v1/builds/{build_id} \
  -H "Authorization: Bearer <session_token>"
```

## Verify

Confirm the build completed successfully:

1. Check the build status in the UI or via `GET /v1/builds/{build_id}`
2. Review the build logs for any errors
3. Check artifacts on the build detail page
