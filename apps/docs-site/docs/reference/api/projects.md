---
status: implemented
description: "API endpoints for managing projects in oore.build."
---

# Projects API

Endpoints for managing projects. All endpoints require a valid user session.

## List Projects {#list-projects}

```
GET /v1/projects
```

**Authentication**: User session (Bearer)

### Response `200 OK`

Returns an array of project objects.

```json
[
  {
    "id": "proj_abc123",
    "name": "My Flutter App",
    "repository_url": "https://github.com/org/my-flutter-app",
    "integration_id": "int_def456",
    "created_at": 1738800000,
    "updated_at": 1738800000
  }
]
```

---

## Create Project {#create-project}

```
POST /v1/projects
```

**Authentication**: User session (Bearer)

### Request body

```json
{
  "name": "My Flutter App",
  "repository_url": "https://github.com/org/my-flutter-app",
  "integration_id": "int_def456"
}
```

### Response `200 OK`

Returns the created project object.

### Error responses

| Status | Code | Description |
|---|---|---|
| 400 | `invalid_input` | Missing or invalid fields |
| 401 | `missing_auth` | Authorization header not provided |
| 401 | `invalid_session` | Session token is invalid |

---

## Get Project {#get-project}

```
GET /v1/projects/{project_id}
```

**Authentication**: User session (Bearer)

### Response `200 OK`

Returns the project object.

---

## Update Project {#update-project}

```
PATCH /v1/projects/{project_id}
```

**Authentication**: User session (Bearer)

### Request body

Partial update — only include fields to change.

```json
{
  "name": "Updated App Name"
}
```

### Response `200 OK`

Returns the updated project object.

---

## Delete Project {#delete-project}

```
DELETE /v1/projects/{project_id}
```

**Authentication**: User session (Bearer, owner/admin)

### Response `200 OK`

Returns a confirmation.

### Error responses

| Status | Code | Description |
|---|---|---|
| 403 | `forbidden` | Insufficient permissions (requires owner or admin role) |
