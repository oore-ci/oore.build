---
title: 'How artifact delivery works'
status: implemented
description: 'Understand how Oore turns runner output into authorized downloads and device install sessions.'
---

An artifact becomes available only after the runner uploads and finalizes it.
Oore then checks project access before issuing a short-lived capability for a
download or mobile installation.

## Storage changes the byte path

Local storage is the default when no supported object-storage configuration
exists. Artifact bytes remain under the backend's managed data root, and
authorized uploads and downloads use tokenized backend routes.

S3 and R2 are supported alternatives. The backend keeps the storage
configuration and authorizes each operation, while the runner or user receives
a time-bound presigned object-store URL. Explicit invalid or disabled storage
configuration fails closed instead of silently selecting local storage.

Local upload tokens are single-use and local uploads use the backend's request
size limit. Those properties do not describe presigned S3 or R2 uploads.

## Availability begins after finalization

The runner first reserves an artifact, uploads its bytes, and then completes or
aborts the reservation. Pending and failed reservations are not listed or
downloadable.

Declared artifact patterns are part of build success. A pipeline with patterns
must produce and finalize a match; a pipeline with no patterns does not require
an artifact.

## Downloads begin with project authorization

Oore checks project artifact-read access before generating a download. The
result is a short-lived URL and expiry:

- local storage returns a signed Oore URL
- S3 or R2 returns a presigned provider URL

Ordinary download links last 15 minutes. The capability can be used without a
second Oore session check, so it should be handled like a temporary bearer
secret. When the artifact itself expires, Oore cannot mint a new link.

## Mobile installation is a separate capability

An install session lasts at most one hour and never outlives the artifact.
Android can deliver an APK over a device-reachable path where the platform and
browser permit it.

iOS over-the-air installation requires HTTPS, an install-ready signed IPA, and
complete app metadata. The phone fetches a generated manifest and then the IPA,
so the same artifact-scoped capability remains valid for both requests within
its limited lifetime.

The copied build or install page still requires Oore sign-in and project
access. When an authorized user selects **Install**, Oore creates the
artifact-scoped delivery capability. The resulting delivery URL grants access
only to that artifact.

## What this means for you

- Object storage is optional; backend-local storage is a supported default.
- A successful upload is not visible until finalization succeeds.
- Sharing a generated download or delivery URL shares its temporary
  capability.
- Expired artifacts require a new build, not a refreshed URL.
- iOS delivery has stricter signing, metadata, device, and HTTPS requirements
  than Android delivery.
- A managed state backup does not automatically include backend-local artifact
  payloads.

## Next step

[Download a build artifact](/build/distribute/download) or
[configure artifact storage](/operate/storage/artifacts). Use the generated
[Artifacts API reference](/reference/api/categories/artifacts) for exact
operations and response fields.
