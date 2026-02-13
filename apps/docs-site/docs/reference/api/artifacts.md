---
status: implemented
description: "API endpoints for build artifact management and downloads in oore.build."
---

# Artifacts API

Endpoints for managing build artifacts and downloads.

## List Artifacts {#list-artifacts}

List artifacts for a specific build.

```
GET /v1/builds/{build_id}/artifacts
```

**Authentication**: User session (Bearer)

### Response `200 OK`

Returns an array of artifact objects.

```json
[
  {
    "id": "art_abc123",
    "build_id": "build_xyz789",
    "filename": "app-release.apk",
    "size_bytes": 15728640,
    "content_type": "application/vnd.android.package-archive",
    "created_at": 1738800360
  }
]
```

---

## Generate Download Link {#generate-download-link}

Generate a time-limited download URL for an artifact.

```
POST /v1/artifacts/{artifact_id}/download-link
```

**Authentication**: User session (Bearer)

### Response `200 OK`

```json
{
  "url": "http://127.0.0.1:8787/v1/artifacts/download/token_abc123",
  "expires_at": 1738800600
}
```

For S3/R2 storage, the URL is a pre-signed URL pointing directly to the storage provider. For local storage, the URL points to the daemon.

---

## Download Artifact {#download-artifact}

Download an artifact using a token from the download link endpoint.

```
GET /v1/artifacts/download/{token}
```

**Authentication**: Download token (in URL path)

Returns the artifact file with appropriate `Content-Type` and `Content-Disposition` headers.

---

## Upload Artifact (Runner) {#upload-artifact}

Used by runners to upload artifacts after a successful build.

```
POST /v1/runners/{runner_id}/jobs/{job_id}/artifacts
```

**Authentication**: Runner token (Bearer)

This endpoint is called by the runner process, not by end users.

---

## Local Upload (Runner) {#local-upload}

For local storage, runners upload artifact files directly to the daemon.

```
PUT /v1/artifacts/local-upload/{token}
```

**Authentication**: Upload token (in URL path)

This endpoint receives the raw file bytes with a size limit defined by the daemon configuration.
