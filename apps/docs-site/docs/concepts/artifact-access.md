---
status: implemented
description: "How Oore CI generates signed download links for build artifacts."
---

# Artifact Access Model

How Oore CI stores, secures, and serves build artifacts.

## Storage backends

Oore CI supports three artifact storage backends:

| Backend | Description | Best for |
|---------|-------------|----------|
| `local` | Files stored on the daemon's filesystem | Development, single-machine setups |
| `s3` | Amazon S3 or S3-compatible storage | Production deployments |
| `r2` | Cloudflare R2 | Production deployments with Cloudflare |

Configure storage via the [Settings API](/reference/api/settings#update-artifact-storage) or the web UI. See the [Configure Storage guide](/guides/artifacts/configure-storage) for step-by-step instructions.

## Upload flow

When a runner produces a build artifact:

1. **Runner creates an artifact record** via `POST /v1/runners/{runner_id}/jobs/{job_id}/artifacts`
   - Provides: artifact name, type (`apk`, `ipa`, `app`, `generic`), file size, and SHA-256 checksum
   - The daemon validates the artifact name (1-255 characters, no path separators) and checks for duplicate checksums within the build

2. **Daemon generates an upload URL**
   - **S3/R2**: A presigned PUT URL with a 30-minute TTL, targeting `artifacts/{build_id}/{artifact_id}/{name}`
   - **Local**: A token-based upload path (`/v1/artifacts/local-upload/{token}`)

3. **Runner uploads the file** to the URL
   - Maximum file size: 512 MiB
   - The upload URL is single-use and time-limited

4. **Runner finalizes the reservation**
   - `POST .../artifacts/{artifact_id}/complete` makes the artifact available
   - `POST .../artifacts/{artifact_id}/abort` records a failed upload
   - Pending and failed artifacts are not listed or downloadable

Declared artifact patterns are part of build success: an empty pattern list requires no artifact, while a non-empty list must produce at least one finalized artifact. Missing matches and upload/finalization failures fail the build.

## Download flow

When a user wants to download an artifact:

1. **User requests a download link** via `POST /v1/artifacts/{artifact_id}/download-link`
   - Requires an authenticated user session with `builds:read` permission
   - The daemon performs an RBAC check before generating the link

2. **Daemon generates a signed download URL**
   - **S3/R2**: A presigned GET URL with a 15-minute TTL
   - **Local**: A token-based download path (`/v1/artifacts/download/{token}`)

3. **User downloads the file** using the signed URL
   - The URL expires after 15 minutes
   - Each download-link request generates a fresh URL

4. **Audit logging**: The daemon records who requested the download link and when

## Signed URL security

Signed URLs are the core security mechanism for artifact access:

| Property | Upload | Download |
|----------|--------|----------|
| **TTL** | 30 minutes | 15 minutes |
| **Auth required to generate** | Runner token | User session + `builds:read` |
| **URL reusable** | No (single-use for S3) | Until expiry |
| **Accessible without session** | Yes (presigned) | Yes (presigned) |

The presigned URL model means:
- **No credentials in URLs**: S3 access keys are never exposed to runners or users
- **Time-limited access**: URLs expire, preventing stale links from being shared indefinitely
- **No proxy bottleneck**: Downloads go directly to S3/R2, not through the daemon

## Local backend tokens

For the `local` storage backend, the daemon manages its own token system:

1. A cryptographic token is generated when an upload or download URL is requested
2. The token hash is stored in memory with an expiry timestamp
3. When the token is used, the daemon validates the hash and checks expiry
4. Expired or invalid tokens return `404 Not Found`

This provides equivalent security to S3 presigned URLs for local deployments.

## Access control layers

Artifact access is protected by multiple layers:

1. **Runner assignment**: Only the runner assigned to a build can create artifacts for that build
2. **RBAC permissions**: Only users with `builds:read` permission can request download links
3. **Signed URLs**: Time-limited, so sharing a link has a bounded exposure window
4. **Audit trail**: Download link generation is logged for accountability
5. **Checksum deduplication**: Prevents duplicate artifact uploads within a build
