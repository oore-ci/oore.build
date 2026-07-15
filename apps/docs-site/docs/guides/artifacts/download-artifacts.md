---
status: implemented
description: 'Download build artifacts from Oore CI using signed time-limited URLs.'
---

# Download Build Artifacts

After a build completes, artifacts (APKs, IPAs, etc.) are available for download through the web UI or API.

## What you need

- **Role**: any authenticated user
- A completed build with artifacts

## Download via UI

1. Open the project in the web UI
2. Go to the **Builds** list
3. Click on a completed build
4. Artifacts appear in the build detail page
5. Click **Download** next to the artifact you want

## Download via API

### 1. List artifacts for a build

```bash
curl http://127.0.0.1:8787/v1/builds/{build_id}/artifacts \
  -H "Authorization: Bearer <session_token>"
```

### 2. Generate a download link

```bash
curl -X POST http://127.0.0.1:8787/v1/artifacts/{artifact_id}/download-link \
  -H "Authorization: Bearer <session_token>"
```

Response:

```json
{
  "url": "http://127.0.0.1:8787/v1/artifacts/download/token_abc123",
  "expires_at": 1738800600
}
```

### 3. Download the file

```bash
curl -o my-app.apk "http://127.0.0.1:8787/v1/artifacts/download/token_abc123"
```

Download links are time-limited. Generate a new link if the previous one expired.

To install an APK or signed ad-hoc IPA directly on a phone, use the artifact's **Install** action instead. See [Install Mobile Builds](/guides/artifacts/install-mobile-builds).

## How download links work

| Storage backend | Download mechanism                            |
| --------------- | --------------------------------------------- |
| **Local**       | Daemon serves the file via a signed token URL |
| **S3**          | Daemon generates a pre-signed S3 URL          |
| **R2**          | Daemon generates a pre-signed R2 URL          |

The download token is short-lived and scoped to the specific artifact. No direct access to the underlying storage is exposed.

## API endpoints

| Method | Path                                        | Description                |
| ------ | ------------------------------------------- | -------------------------- |
| `GET`  | `/v1/builds/{build_id}/artifacts`           | List artifacts for a build |
| `POST` | `/v1/artifacts/{artifact_id}/download-link` | Generate download link     |
| `GET`  | `/v1/artifacts/download/{token}`            | Download artifact          |
