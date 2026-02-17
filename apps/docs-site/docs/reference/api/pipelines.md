---
status: implemented
description: "API endpoints for pipeline configuration and validation in Oore CI."
---

# Pipelines API

Endpoints for managing pipeline configuration and signing settings. All endpoints require a valid user session.

## List Pipelines {#list-pipelines}

```
GET /v1/projects/{project_id}/pipelines
```

**Authentication**: User session (Bearer)

### Response `200 OK`

Returns an array of pipeline objects for the project.

```json
[
  {
    "id": "pipe_abc123",
    "project_id": "proj_def456",
    "name": "Android Release",
    "config_path": ".oore.yaml",
    "config_path_explicit": false,
    "execution_config": { ... },
    "trigger_config": { "events": ["push"], "branches": ["main"] },
    "concurrency": { "max_concurrent": 1, "cancel_in_progress": false },
    "enabled": true,
    "created_at": 1738800000,
    "updated_at": 1738800000
  }
]
```

---

## Create Pipeline {#create-pipeline}

```
POST /v1/projects/{project_id}/pipelines
```

**Authentication**: User session (Bearer)

### Request body

```json
{
  "name": "Android Release",
  "execution_config": {
    "platforms": ["android"],
    "flutter_version": "3.24.0",
    "commands": {
      "pre_build": ["flutter pub get"],
      "build": ["flutter build apk --release"],
      "post_build": []
    },
    "artifact_patterns": ["**/*.apk"]
  },
  "trigger_config": {
    "events": ["push"],
    "branches": ["main"]
  }
}
```

### Response `200 OK`

Returns the created pipeline object.

---

## Get Pipeline {#get-pipeline}

```
GET /v1/pipelines/{pipeline_id}
```

**Authentication**: User session (Bearer)

### Response `200 OK`

Returns the pipeline object with full execution config.

---

## Update Pipeline {#update-pipeline}

```
PATCH /v1/pipelines/{pipeline_id}
```

**Authentication**: User session (Bearer)

### Request body

Partial update — only include fields to change.

---

## Delete Pipeline {#delete-pipeline}

```
DELETE /v1/pipelines/{pipeline_id}
```

**Authentication**: User session (Bearer)

### Response `200 OK`

Returns a confirmation.

---

## Validate Pipeline Config {#validate-pipeline}

Validate a pipeline configuration without creating it.

```
POST /v1/pipelines/validate
```

**Authentication**: User session (Bearer)

### Request body

```json
{
  "config": "version: 1\nplatforms:\n  - android\n..."
}
```

### Response `200 OK`

Returns validation results. Errors are returned inline, not as HTTP errors.

---

## Pipeline Signing

### Get Android Signing {#get-android-signing}

```
GET /v1/pipelines/{pipeline_id}/android-signing
```

Returns the Android signing configuration for a pipeline.

### Update Android Signing {#update-android-signing}

```
PUT /v1/pipelines/{pipeline_id}/android-signing
```

Set or update Android keystore signing configuration.

### Get iOS Signing {#get-ios-signing}

```
GET /v1/pipelines/{pipeline_id}/ios-signing
```

Returns the iOS signing configuration for a pipeline.

### Update iOS Signing {#update-ios-signing}

```
PUT /v1/pipelines/{pipeline_id}/ios-signing
```

Set or update iOS signing configuration (manual or API mode).

### Sync iOS Signing {#sync-ios-signing}

```
POST /v1/pipelines/{pipeline_id}/ios-signing/sync
```

Sync certificates and profiles from App Store Connect (API mode only).

### List iOS Devices {#list-ios-devices}

```
GET /v1/pipelines/{pipeline_id}/ios-signing/devices
```

List registered iOS test devices for this pipeline.

### Register iOS Device {#register-ios-device}

```
POST /v1/pipelines/{pipeline_id}/ios-signing/devices/register
```

Register a new test device for ad hoc distribution.
