---
status: implemented
description: 'Install Android APK and signed iOS ad-hoc builds on test devices from Oore CI.'
---

# Install Mobile Builds

Oore gives authenticated team members a device-first install page for Android APK and signed iOS ad-hoc IPA artifacts. QA Viewers can install builds without pipeline, signing, or share-link management access.

## What you need

- A completed build with an APK or signed ad-hoc IPA artifact
- An External Access public URL or separate Artifact delivery URL configured for the Oore instance
- For iOS, an HTTPS delivery URL
- For iOS, the phone's UDID included in the provisioning profile used by the build
- For iOS, Safari on the iPhone
- For iOS, Developer Mode enabled before opening the installed app

## Open the install page

1. Open **Builds** in Oore.
2. Select a completed build.
3. Find the APK or install-ready IPA under **Artifacts**.
4. Select **Install**.
5. Copy the install-page link if you need to move from a desktop browser to the phone.

The install session is scoped to one artifact and expires after one hour or when the artifact expires, whichever happens first.

## Instances behind an interactive auth proxy

Apple fetches the manifest and IPA outside the signed-in browser page. If the main Oore URL is protected by Warpgate or another proxy that redirects every unauthenticated request to a login page, configure a separate **Artifact delivery URL** under **Settings → Preferences → External Access**.

Route only token-authenticated `GET` and `HEAD` requests for these paths from that HTTPS origin to `oore-web`:

- `/v1/artifacts/install/ios/`
- `/v1/artifacts/dl/`
- `/v1/artifacts/download/`

Keep every other Oore path behind the normal identity proxy. The delivery URLs are bearer credentials, expire within one hour, and remain scoped to one artifact. A request with an invalid or expired token is rejected by Oore.

## Install on Android

1. Open the install page on the Android phone.
2. Select **Install APK**.
3. If Android asks, allow the current browser or file manager to install unknown apps.
4. Open the downloaded APK and confirm installation.

Android controls unknown-app permission per source. You can disable the permission again after installation.

## Install on iPhone

1. Open the install page in Safari on the iPhone.
2. Select **Install on iPhone**.
3. Confirm the installation prompt.
4. Wait for the app icon to finish installing.
5. Enable Developer Mode if iOS asks, then open the app.

Oore detects common non-Safari iPhone browsers and disables the install action with an **Open this page in Safari** notice. Downloading an IPA directly does not install it.

Apple ad-hoc distribution only installs on devices included in the app's provisioning profile, and Apple requires Developer Mode to run an IPA-based app. Confirm that the exact device UDID was registered before the build. See [Register iOS Test Devices](/guides/signing/ios-device-registration) and Apple's [registered-device distribution guide](https://developer.apple.com/documentation/xcode/distributing-your-app-to-registered-devices).

## Why an older IPA may show “Not install-ready”

Oore's OTA manifest requires the app's bundle identifier, display name, version, and build number. Newly produced signed IPAs attach this metadata automatically. An IPA built by an older runner remains downloadable, but it must be rebuilt once with the current runner before Oore can create the install manifest.

The IPA must also use Oore's `ad-hoc` or `release-testing` export path and have a provisioning profile matching the app bundle identifier.

## Troubleshooting

| Symptom                              | What to check                                                                                     |
| ------------------------------------ | ------------------------------------------------------------------------------------------------- |
| Install button says to use Safari    | Copy the page URL and open it in Safari on the iPhone                                             |
| iOS install prompt never appears     | Confirm the delivery URL uses HTTPS, is install-ready, and does not redirect token paths to login |
| iOS says the app cannot be installed | Confirm the phone UDID is in the profile used for this exact build and the profile is not expired |
| iOS app installs but will not open   | Enable Developer Mode, then retry                                                                 |
| Android blocks the APK               | Allow unknown-app installation for the browser or file manager that opened the APK                |
| Artifact is expired                  | Run a new build; expired artifacts cannot mint install sessions                                   |

## Access rules

- Owner, Admin, Developer, and QA Viewer roles can install and download artifacts they can read.
- QA Viewers cannot edit pipelines, inspect signing configuration/device inventory, trigger builds, or create reusable external share links.
- Developers and instance administrators can create and revoke external share links.

Installation links are bearer URLs after creation. Do not forward them outside the intended test group.
