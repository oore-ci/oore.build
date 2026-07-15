---
status: implemented
description: 'API endpoints for build artifact management and downloads in Oore CI.'
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

## Create Install Link {#create-install-link}

Create a one-hour device installation session for an APK or install-ready signed IPA.

```
POST /v1/artifacts/{artifact_id}/install-link
```

**Authentication**: User session with artifact read access (Bearer)

### iOS response `200 OK`

```json
{
  "platform": "ios",
  "install_url": "itms-services://?action=download-manifest&url=https%3A%2F%2Finstall.ci.example.com%2Fv1%2Fartifacts%2Finstall%2Fios%2Ftoken%2Fmanifest.plist",
  "download_url": "https://install.ci.example.com/v1/artifacts/dl/token",
  "manifest_url": "https://install.ci.example.com/v1/artifacts/install/ios/token/manifest.plist",
  "expires_at": 1784073600
}
```

APK responses use `platform: "android"`, set `install_url` to the scoped APK download URL, and omit `manifest_url`.

The endpoint prefers the optional Artifact delivery URL and otherwise uses the External Access public URL. It returns `412` if neither is available or iOS does not have an HTTPS delivery URL. It returns `422` for unsupported artifacts or signed IPAs missing current install metadata.

---

## iOS Install Manifest {#ios-install-manifest}

Return the Apple OTA property-list manifest referenced by an iOS install URL.

```
GET /v1/artifacts/install/ios/{token}/manifest.plist
```

**Authentication**: Install token (in URL path)

The XML manifest identifies the app and references `/v1/artifacts/dl/{token}` for the protected IPA download. The token remains reusable until expiry because iOS fetches the manifest and IPA separately.

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

The returned artifact is `pending`. After uploading, the runner must call:

```
POST /v1/runners/{runner_id}/jobs/{job_id}/artifacts/{artifact_id}/complete
```

If upload fails, it calls the corresponding `/abort` endpoint. Only completed (`available`) artifacts appear in list and download APIs.

---

## Local Upload (Runner) {#local-upload}

For local storage, runners upload artifact files directly to the daemon.

```
PUT /v1/artifacts/local-upload/{token}
```

**Authentication**: Upload token (in URL path)

This endpoint receives the raw file bytes with a size limit defined by the daemon configuration.
