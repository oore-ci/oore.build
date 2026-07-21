---
status: implemented
description: 'API endpoints for managing projects in Oore CI.'
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
    "repository_id": "repo_def456",
    "repository_full_name": "org/my-flutter-app",
    "repository_provider": "github",
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

**Authentication**: owner or admin user session

### Request body

```json
{
  "name": "My Flutter App",
  "repository_id": "repo_def456"
}
```

### Response `200 OK`

Returns the created project object.

Creating a project links executable repository code to the Direct runner trust
boundary. Only Owners and Admins may make this decision.

### Error responses

| Status | Code                | Description                       |
| ------ | ------------------- | --------------------------------- |
| 400    | `invalid_input`     | Missing or invalid fields         |
| 401    | `missing_auth`      | Authorization header not provided |
| 401    | `invalid_session`   | Session token is invalid          |
| 403    | `insufficient_role` | Owner or Admin role is required   |

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

Changing `repository_id` requires an Owner or Admin because it changes which
repository is trusted to execute. Other project updates continue to use project
RBAC. The same action is available in **Project > Settings > Source repository**
for repairing a source link removed by an integration sync or the one-time
execution-trust migration.

---

## Delete Project {#delete-project}

```
DELETE /v1/projects/{project_id}
```

**Authentication**: User session (Bearer, owner/admin)

### Response `200 OK`

Returns a confirmation.

### Error responses

| Status | Code        | Description                                             |
| ------ | ----------- | ------------------------------------------------------- |
| 403    | `forbidden` | Insufficient permissions (requires owner or admin role) |
