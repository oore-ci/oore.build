---
status: implemented
description: "API endpoints for instance settings, External Access preflight, and runtime preferences."
---

# Settings API

Endpoints for managing instance-level configuration.

## Get Artifact Storage Settings {#get-artifact-storage}

```
GET /v1/settings/artifact-storage
```

**Authentication**: User session (Bearer, read access to `instance_settings`)

### Response `200 OK`

Returns `ArtifactStorageSettingsResponse`.

---

## Update Artifact Storage Settings {#update-artifact-storage}

```
PUT /v1/settings/artifact-storage
```

**Authentication**: User session (Bearer, write access to `instance_settings`)

### Request body

Uses `UpdateArtifactStorageSettingsRequest`.

### Error responses

| Status | Code | Description |
|---|---|---|
| 400 | `invalid_local_base_dir` / `invalid_s3_bucket` / `invalid_s3_endpoint` / `missing_s3_credentials` | Provider-specific validation failed |
| 403 | `insufficient_role` | Caller lacks write permission |

---

## Get Instance Preferences {#get-preferences}

```
GET /v1/settings/preferences
```

**Authentication**: User session (Bearer, read access to `instance_settings`)

### Response `200 OK`

```json
{
  "preferences": {
    "key_storage_mode": "file",
    "runtime_mode": "local",
    "restart_required": true,
    "updated_at": 1738886400
  }
}
```

---

## External Access Preflight {#external-access-preflight}

```
GET /v1/settings/external-access/preflight
```

Returns check-by-check readiness required before enabling External Access (`runtime_mode=remote`).

**Authentication**: User session (Bearer, read access to `instance_settings`)

### Response `200 OK`

```json
{
  "ready": false,
  "checks": [
    {
      "id": "public_url_https",
      "label": "Public URL is configured with HTTPS",
      "ok": false,
      "message": "OORE_PUBLIC_URL must use https for External Access.",
      "failure_code": "external_access_https_required"
    }
  ]
}
```

---

## Update Instance Preferences {#update-preferences}

```
PUT /v1/settings/preferences
```

**Authentication**: User session (Bearer, write access to `instance_settings`)

### Request body

```json
{
  "key_storage_mode": "file",
  "runtime_mode": "remote"
}
```

### Runtime mode mutation rules

- Changing `runtime_mode` is owner-only.
- Enabling `runtime_mode=remote` runs hard External Access preflight and fails closed if any required check fails.
- Any runtime mode change revokes all active sessions.

### Error responses

| Status | Code | Description |
|---|---|---|
| 400 | `unsupported_key_storage_mode` | Only `file` mode is allowed in this release |
| 400 | `external_access_preflight_failed` | Generic preflight failure |
| 400 | `external_access_public_url_missing` | Public URL is missing/invalid/loopback |
| 400 | `external_access_https_required` | Public URL is not HTTPS |
| 400 | `external_access_origin_not_allowed` | Public origin not allowlisted in CORS |
| 403 | `external_access_owner_required` | Non-owner attempted runtime mode change |
