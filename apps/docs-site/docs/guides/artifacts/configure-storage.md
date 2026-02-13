---
status: implemented
description: "Configure artifact storage backends for oore.build including local, S3, and R2."
---

# Configure Artifact Storage

Build artifacts (APKs, IPAs, etc.) need a storage location. oore.build supports local filesystem storage, Amazon S3, and Cloudflare R2.

## What you need

- **Role**: owner or admin
- A running oore.build instance in `ready` state

## Storage options

| Backend | Best for | Requirements |
|---|---|---|
| **Local** | Single-host setups, development | Daemon filesystem access |
| **S3** | Production, multi-region | AWS account, S3 bucket |
| **R2** | Cost-effective production | Cloudflare account, R2 bucket |
| **Disabled** | No artifact storage needed | — |

## Configure via UI

1. Go to **Settings > Artifact Storage** in the web UI
2. Select a storage backend
3. Enter the required configuration
4. Click **Save**

### Local storage

No additional configuration needed. Artifacts are stored on the daemon's filesystem and served via signed token URLs.

### S3 storage

| Field | Description |
|---|---|
| **Bucket name** | S3 bucket name |
| **Region** | AWS region (e.g., `us-east-1`) |
| **Access key ID** | AWS access key |
| **Secret access key** | AWS secret key |
| **Prefix** | Optional path prefix within the bucket |

Create an S3 bucket following the [AWS S3 documentation](https://docs.aws.amazon.com/AmazonS3/latest/userguide/create-bucket-overview.html). The IAM credentials need `s3:PutObject`, `s3:GetObject`, and `s3:DeleteObject` permissions on the bucket.

### R2 storage

| Field | Description |
|---|---|
| **Bucket name** | R2 bucket name |
| **Account ID** | Cloudflare account ID |
| **Access key ID** | R2 API token access key |
| **Secret access key** | R2 API token secret key |

Create an R2 bucket following the [Cloudflare R2 documentation](https://developers.cloudflare.com/r2/get-started/).

## Configure via API

```bash
curl -X PUT http://127.0.0.1:8787/v1/settings/artifact-storage \
  -H "Authorization: Bearer <session_token>" \
  -H "Content-Type: application/json" \
  -d '{
    "backend": "s3",
    "config": {
      "bucket": "my-oore-artifacts",
      "region": "us-east-1",
      "access_key_id": "AKIA...",
      "secret_access_key": "..."
    }
  }'
```

## Verify

1. Trigger a build that produces artifacts
2. After the build succeeds, click on the build in the UI
3. Artifacts should appear with download links
4. Click a download link to verify it works

## API endpoints

| Method | Path | Description |
|---|---|---|
| `GET` | `/v1/settings/artifact-storage` | Get current storage config |
| `PUT` | `/v1/settings/artifact-storage` | Update storage config |
