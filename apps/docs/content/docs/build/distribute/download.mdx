---
title: 'Download a build artifact'
status: implemented
description: 'Download an available artifact through a short-lived, storage-backed URL.'
---

Download one available artifact from a build. Oore authorizes access first,
then returns a short-lived URL for the configured storage backend.

## What you need

- Project viewer, developer, maintainer, instance admin, or instance owner
  access to the build.
- An artifact whose state is available and whose retention period has not
  expired.

## 1. Open the artifact

Open the project, select the completed build, and find **Artifacts**.

For an ordinary artifact, select **Download** beside its name. For an
install-ready APK or IPA, **Install** opens the device-delivery page instead;
use the [Android](/build/distribute/install/android) or
[iOS](/build/distribute/install/ios) task.

## 2. Use the generated URL

Oore returns:

```json
{
  "download_url": "https://artifacts.example.com/signed-path",
  "expires_at": 1738800600
}
```

`expires_at` is a Unix timestamp. An ordinary download URL lasts 15 minutes.
Local storage returns an Oore signed-token URL; S3 and R2 return a presigned
object URL.

The HTTP operations are
[List build artifacts](/openapi/operations/list_artifacts) and
[Generate download link](/openapi/operations/generate_download_link).

## Verify the result

The browser saves a non-empty file whose name and size match the artifact
shown in Oore.

## Troubleshooting

**The artifact has expired**

An expired artifact returns `410` with `artifact_expired`. Oore cannot mint a
new URL for it; run a new build when the file is still needed.

**The generated URL has expired**

Return to the still-available artifact and request another download. Do not
reuse the 15-minute URL.

**The artifact is hidden or returns not found**

Confirm that your account has project access. Oore returns a privacy-preserving
not-found result when the project is not visible.

## Next step

For mobile testing, open the device-specific
[Android](/build/distribute/install/android) or
[iOS](/build/distribute/install/ios) install task.
