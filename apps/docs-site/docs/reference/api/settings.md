---
status: implemented
---

# Settings API

Endpoints for managing instance-level configuration.

## Get Artifact Storage Settings {#get-artifact-storage}

```
GET /v1/settings/artifact-storage
```

**Authentication**: User session (Bearer)

### Response `200 OK`

```json
{
  "backend": "s3",
  "config": {
    "bucket": "my-oore-artifacts",
    "region": "us-east-1"
  }
}
```

Backend values: `disabled`, `local`, `s3`, `r2`

---

## Update Artifact Storage Settings {#update-artifact-storage}

```
PUT /v1/settings/artifact-storage
```

**Authentication**: User session (Bearer, owner/admin)

### Request body

```json
{
  "backend": "s3",
  "config": {
    "bucket": "my-oore-artifacts",
    "region": "us-east-1",
    "access_key_id": "AKIA...",
    "secret_access_key": "..."
  }
}
```

### Error responses

| Status | Code | Description |
|---|---|---|
| 400 | `invalid_input` | Invalid backend or config |
| 403 | `forbidden` | Insufficient permissions |

---

## Get Instance Preferences {#get-preferences}

```
GET /v1/settings/preferences
```

**Authentication**: User session (Bearer)

### Response `200 OK`

Returns instance-level preferences.

---

## Update Instance Preferences {#update-preferences}

```
PUT /v1/settings/preferences
```

**Authentication**: User session (Bearer, owner/admin)

### Error responses

| Status | Code | Description |
|---|---|---|
| 403 | `forbidden` | Insufficient permissions |
