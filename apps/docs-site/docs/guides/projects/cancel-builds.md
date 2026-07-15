---
status: implemented
description: 'Cancel running or queued builds in Oore CI.'
---

# Cancel Running Builds

Cancel a build that is in `queued` or `running` state.

## What you need

- **Role**: developer, admin, or owner
- A build in `queued` or `running` state

## Cancel via UI

1. Open the project in the web UI
2. Go to the **Builds** list
3. Click on the running or queued build
4. Click **Cancel Build**

The build transitions to `canceled` state.

## Cancel via API

```bash
curl -X POST http://127.0.0.1:8787/v1/builds/{build_id}/cancel \
  -H "Authorization: Bearer <session_token>"
```

### Response

Returns the updated build object with `status: "canceled"`.

Canceling a build that has already reached a terminal state (succeeded, failed, timed_out, expired) has no effect.

## What happens when a build is canceled

- If **queued**: The build is removed from the queue and marked as `canceled`
- If **running**: The runner is signaled to stop execution. The build is marked as `canceled` after the runner confirms.
- Build logs up to the point of cancellation are preserved
- No artifacts are collected from canceled builds
