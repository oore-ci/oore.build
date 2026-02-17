---
status: implemented
description: "API endpoints for user management and invitations in Oore CI."
---

# Users API

Endpoints for user management. All endpoints require a valid user session.

## Get Current User {#get-me}

```
GET /v1/users/me
```

**Authentication**: User session (Bearer)

### Response `200 OK`

```json
{
  "id": "user_abc123",
  "email": "dev@example.com",
  "display_name": "Jane Developer",
  "role": "developer",
  "status": "active",
  "avatar_url": "https://lh3.googleusercontent.com/...",
  "created_at": 1738800000,
  "updated_at": 1738800000
}
```

---

## List Users {#list-users}

```
GET /v1/users
```

**Authentication**: User session (Bearer, owner/admin)

### Response `200 OK`

Returns an array of user objects.

---

## Invite User {#invite-user}

```
POST /v1/users/invite
```

**Authentication**: User session (Bearer, owner/admin)

### Request body

```json
{
  "email": "dev@example.com",
  "role": "developer"
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `email` | `string` | Yes | Email address (must match OIDC provider account) |
| `role` | `string` | Yes | Role to assign: `admin`, `developer`, or `qa_viewer` |

### Response `200 OK`

Returns the created user object with `invited` status.

### Error responses

| Status | Code | Description |
|---|---|---|
| 400 | `invalid_input` | Invalid email or role |
| 403 | `forbidden` | Insufficient permissions |
| 409 | `already_exists` | User with this email already exists |

---

## Update User Role {#update-user-role}

```
PATCH /v1/users/{user_id}/role
```

**Authentication**: User session (Bearer, owner/admin)

### Request body

```json
{
  "role": "admin"
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `role` | `string` | Yes | New role: `admin`, `developer`, or `qa_viewer` |

### Response `200 OK`

Returns the updated user object.

### Error responses

| Status | Code | Description |
|---|---|---|
| 400 | `invalid_input` | Invalid role value |
| 403 | `forbidden` | Insufficient permissions |
| 404 | `not_found` | User not found |

---

## Disable User {#disable-user}

Disable a user account. The user's sessions are invalidated and they cannot sign in.

```
DELETE /v1/users/{user_id}
```

**Authentication**: User session (Bearer, owner/admin)

### Response `200 OK`

Returns the updated user object with `disabled` status.

### Error responses

| Status | Code | Description |
|---|---|---|
| 403 | `forbidden` | Insufficient permissions |
| 404 | `not_found` | User not found |

::: info
This endpoint disables the user rather than permanently deleting them. User records are preserved for the audit trail.
:::

---

## Re-enable User {#re-enable-user}

Re-enable a previously disabled user account.

```
POST /v1/users/{user_id}/enable
```

**Authentication**: User session (Bearer, owner/admin)

### Response `200 OK`

Returns the updated user object with `active` status.

### Error responses

| Status | Code | Description |
|---|---|---|
| 403 | `forbidden` | Insufficient permissions |
| 404 | `not_found` | User not found |
