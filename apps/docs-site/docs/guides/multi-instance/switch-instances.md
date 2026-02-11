---
status: implemented
---

# Switch Between Instances

When you have multiple oore.build backend instances connected, you can switch between them without losing your session on either.

## What you need

- At least two instances [added](/guides/multi-instance/add-instance) in the web UI

## Switch instances

1. Click the **instance switcher** in the navigation bar
2. Select the instance you want to switch to
3. The UI reloads with the selected instance's data

## What happens when you switch

| Aspect | Behavior |
|---|---|
| **Authentication** | Each instance has its own session. If you're not signed in to the target instance, you'll be redirected to sign in. |
| **Data** | Projects, builds, pipelines, and settings are loaded from the selected instance. |
| **Query cache** | The cache is partitioned by instance ID — switching instances loads a separate cache. |
| **URL** | API requests are routed to the selected instance's backend URL. |

## Session persistence

Sessions persist independently per instance. If you:

1. Sign in to Instance A
2. Switch to Instance B and sign in
3. Switch back to Instance A

You'll still be signed in to Instance A — no re-authentication needed (unless the session expired).

## No-instance state

If all instances are removed (or on first visit), the UI shows a "Connect an instance" prompt. You must add at least one instance to use the application.
