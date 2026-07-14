---
status: implemented
description: 'Add additional Oore CI backend instances to the web UI.'
---

# Connect to Multiple Backends

The Oore CI web UI supports connecting to multiple backend instances simultaneously. Each instance has its own isolated session, query cache, and data.

## What you need

- The Oore CI web UI running (at `ci.oore.build` or self-hosted)
- URLs of the Oore CI backend instances you want to connect to
- User accounts on each instance

## Add an instance

1. Open the web UI
2. Click the **instance switcher** in the navigation (shows your current instance name or "No instance")
3. Click **Add Instance**
4. Enter:
   - **Label**: A friendly name (e.g., "Production", "Staging")
   - **URL**: The backend daemon URL (e.g., `https://ci.mycompany.com:8787`)
5. Click **Add**

The UI validates the connection by checking `GET /v1/public/setup-status` on the new instance. If the instance is reachable and in `ready` state, it's added to your instance list.

## How instances are stored

Instance records are stored in your browser's `localStorage`:

| Field     | Description                        |
| --------- | ---------------------------------- |
| `id`      | Auto-generated UUID                |
| `label`   | Your display name for the instance |
| `url`     | Backend URL                        |
| `addedAt` | Timestamp when added               |

No instance data is shared between backends or sent to other instances.

## Session isolation

Each instance has a completely independent:

- **Authentication session** — stored in namespaced `sessionStorage`
- **Query cache** — TanStack Query cache partitioned by instance ID
- **API routing** — all API calls include the instance's base URL

Signing in to one instance does not affect your session on another.

## Remove an instance

1. Open the instance switcher
2. Click the **remove** button next to the instance you want to disconnect

This removes the instance from your local registry. It does not affect your account on the backend.

## Next step

[Switch between instances](/guides/multi-instance/switch-instances)
