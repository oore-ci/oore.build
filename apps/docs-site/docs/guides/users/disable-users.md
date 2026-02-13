---
status: implemented
description: "Disable or remove user accounts from your oore.build instance."
---

# Disable and Re-enable Users

Disable user accounts to revoke access without deleting them. Disabled users can be re-enabled later.

## What you need

- **Role**: owner or admin

## Disable a user

### Via UI

1. Go to **Settings > Users**
2. Find the user to disable
3. Click **Disable** (or the disable action on the user row)

The user's status changes to `disabled`. Their existing sessions are invalidated and they can no longer sign in.

### Via API

```bash
curl -X DELETE http://127.0.0.1:8787/v1/users/{user_id} \
  -H "Authorization: Bearer <session_token>"
```

::: info
The `DELETE` endpoint disables the user rather than permanently deleting them. User records are preserved for audit trail purposes.
:::

## Re-enable a user

### Via UI

1. Go to **Settings > Users**
2. Find the disabled user
3. Click **Enable**

### Via API

```bash
curl -X POST http://127.0.0.1:8787/v1/users/{user_id}/enable \
  -H "Authorization: Bearer <session_token>"
```

The user's status returns to `active` and they can sign in again.

## What happens when a user is disabled

- All active sessions are invalidated immediately
- The user cannot start new OIDC authentication flows
- The user's role and data are preserved
- Audit log entries are retained
- The user can be re-enabled at any time

## API endpoints

| Method | Path | Description |
|---|---|---|
| `DELETE` | `/v1/users/{user_id}` | Disable a user |
| `POST` | `/v1/users/{user_id}/enable` | Re-enable a user |
