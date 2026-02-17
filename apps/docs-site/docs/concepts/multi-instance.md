---
status: implemented
description: "How the Oore CI web UI manages connections to multiple backend instances."
---

# Multi-Instance Architecture

The Oore CI web UI supports connecting to multiple backend instances from a single browser session. This page explains how instance isolation works and why it's designed this way.

## Why multi-instance?

Teams often run separate Oore CI instances for different purposes:

- **Production** and **staging** environments
- Separate instances per **team** or **department**
- **Client projects** where each client has their own instance

Rather than signing in and out of different URLs, the web UI maintains simultaneous connections to multiple backends.

## How it works

### Instance registry

The UI maintains a list of connected instances in the browser's `localStorage`. Each instance record contains:

| Field | Description |
|---|---|
| `id` | Auto-generated UUID |
| `label` | User-chosen display name |
| `url` | Backend daemon URL |
| `addedAt` | Timestamp |

### Session isolation

Each instance has a completely independent authentication session:

- **Session tokens** are stored in `sessionStorage`, namespaced by instance ID
- Signing in to one instance has no effect on sessions for other instances
- Sessions expire independently (24-hour TTL per the OIDC session policy)

### API routing

When the user selects an instance, all API requests are routed to that instance's base URL. The API client prepends the instance URL to every request path:

- In development (proxy mode), the base URL is empty (requests go through the Vite dev proxy)
- In production, the base URL is the full daemon URL (e.g., `https://ci.mycompany.com:8787`)

### Query cache partitioning

The TanStack Query cache is partitioned by instance ID. This means:

- Switching instances loads a fresh cache (no cross-instance data contamination)
- Switching back to a previously visited instance restores its cached data
- Each instance's data is independently managed (background refetch, invalidation, etc.)

## Security implications

- No instance can see or access data from another instance
- Session tokens are never shared between instances
- The instance registry itself contains no sensitive data (just labels and URLs)
- All authentication happens directly between the browser and each backend — the hosted UI at `ci.oore.build` never sees session tokens

## Limitations

- Instance management is per-browser (stored in localStorage)
- There is no server-side instance registry or sync
- If localStorage is cleared, all instance connections must be re-added
