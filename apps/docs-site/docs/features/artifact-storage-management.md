# Artifact Storage Management

Owners and admins can configure artifact storage directly from the web UI at **Settings -> Preferences**.

## Supported providers

- `disabled` -- metadata only, binary uploads/downloads disabled
- `local` -- binaries stored on the daemon host filesystem
- `s3` -- AWS S3 / MinIO / generic S3-compatible endpoint
- `r2` -- Cloudflare R2 (S3-compatible)

## What gets configured

- Provider selection
- Local base directory (for local backend)
- Bucket/region/endpoint (for S3/R2)
- Access key ID and secret key (encrypted at rest)

## Security model

- Only `owner` and `admin` can modify settings.
- Credentials are encrypted using daemon AES key material.
- Read APIs expose only boolean credential presence flags.
- Local upload/download links are short-lived signed URLs.

## Runtime behavior

- Changes apply without restarting `oored`.
- Build artifact registration always persists metadata.
- Binary upload/download availability follows active provider.

## API

- `GET /v1/settings/artifact-storage`
- `PUT /v1/settings/artifact-storage`

These APIs are consumed by the Settings UI and can also be used by operators directly.
