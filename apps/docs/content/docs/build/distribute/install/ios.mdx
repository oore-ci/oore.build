---
title: 'Install an iOS build'
status: implemented
description: 'Install an ad hoc iOS build on a registered device through Oore.'
---

Install an available, install-ready IPA on a registered iPhone. After an
authorized user starts the install, Oore creates an Apple over-the-air manifest
and one artifact-scoped capability that remains valid for the manifest and IPA
requests.

## What you need

- Project access that allows artifact downloads.
- A current-runner IPA exported as `ad-hoc` or `release-testing`.
- Complete bundle identifier, app name, version, and build-number metadata.
- The iPhone's UDID included in the provisioning profile used by this build.
- An HTTPS Artifact delivery URL or External Access public URL.
- Safari on the iPhone.

## 1. Open the install page

1. Open the completed build.
2. Under **Artifacts**, select **Install** for the IPA.
3. Select **Copy install page link** and open that link in Safari on the
   registered iPhone.

The copied build-page URL requires Oore sign-in and project access. Selecting
**Install** on that page creates the bearer capability for the manifest and
IPA. The capability lasts up to one hour and ends sooner when the artifact
expires. Do not forward its resulting delivery URL outside the intended test
group.

## 2. Install on the iPhone

1. In Safari, select **Install**.
2. Confirm the iOS installation prompt.
3. Wait for the app icon to finish installing.
4. Enable Developer Mode if iOS asks, then open the app.

Downloading an IPA file directly does not install it.

## Verify the result

The application finishes installing, appears on the Home Screen, and opens on
the registered iPhone.

## Troubleshooting

**The page says Open this page in Safari**

Copy the same install-page URL into Safari on the iPhone. Oore disables the
install action in unsupported iPhone browsers.

**The artifact says Not install-ready**

Rebuild it with the current runner after confirming the `ad-hoc` or
`release-testing` export method and complete app metadata. Older IPAs remain
downloadable but cannot supply the required manifest.

**iOS says the app cannot be installed**

Confirm this exact iPhone UDID is in the provisioning profile used by this
build and that the profile has not expired. Registering a device after the
build does not update the already-built IPA.

**The application installs but will not open**

Enable Developer Mode on the device, then try again.

**The install session expired**

Return to the available artifact and create a fresh install session. If the
artifact has expired, run a new build.

## Next step

Use [Register an iOS test device](/build/distribute/ios-devices) before the next
build when adding another tester.
