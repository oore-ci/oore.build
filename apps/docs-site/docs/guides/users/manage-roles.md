---
status: implemented
description: "Assign and manage user roles and permissions in Oore CI."
---

# Manage User Roles

Change a user's role to grant or restrict their permissions.

## What you need

- **Role**: owner or admin
- The user must already exist in the system (invited or active)

## Steps

### 1. Open user details

Go to **Settings > Users** and find the user whose role you want to change.

### 2. Change the role

Click the user's current role and select the new role from the dropdown:

| Role | Permissions |
|---|---|
| **Admin** | Manage users, settings, projects, pipelines, builds |
| **Developer** | Create/manage projects, trigger builds, download artifacts |
| **QA Viewer** | View builds and download artifacts only |

### 3. Save

The role change takes effect immediately. The user's next API request will use the new permissions.

::: warning
Only the owner can change another admin's role. Admins can change developer and QA viewer roles.
:::

### Via API

```bash
curl -X PATCH http://127.0.0.1:8787/v1/users/{user_id}/role \
  -H "Authorization: Bearer <session_token>" \
  -H "Content-Type: application/json" \
  -d '{"role": "admin"}'
```

### Role values

| API value | Display name |
|---|---|
| `owner` | Owner |
| `admin` | Admin |
| `developer` | Developer |
| `qa_viewer` | QA Viewer |

::: info
The `owner` role cannot be assigned via the role change endpoint. There is exactly one owner per instance (created during setup).
:::

## API endpoints

| Method | Path | Description |
|---|---|---|
| `PATCH` | `/v1/users/{user_id}/role` | Change user role |
