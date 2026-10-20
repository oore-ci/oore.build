---
title: 'Register an iOS test device'
status: implemented
description: 'Register a device through App Store Connect and synchronize its provisioning profiles.'
---

Register one iPhone or iPad for a pipeline that uses App Store Connect signing.
Oore sends the device to Apple and then attempts to synchronize the pipeline's
profiles.

## What you need

- Project developer, maintainer, instance admin, or instance owner access.
- [App Store Connect signing](/build/sign/ios/app-store-connect) in API or
  hybrid mode for the pipeline.
- The device name and UDID.

## 1. Find the UDID

On a Mac, use either method:

- Connect the device, open Finder, select the device, and select the device
  information until **UDID** appears.
- Open Xcode, choose **Window > Devices and Simulators**, select the device,
  and copy **Identifier**.

## 2. Register the device

1. Open the project and edit the pipeline.
2. Find **Registered iOS Devices**.
3. Enter **Device name** and **UDID**.
4. Select **Register Device**.

Oore registers the device through App Store Connect. In API or hybrid mode it
also attempts to synchronize provisioning profiles after registration.
For automation, use the generated
[List registered iOS devices](/openapi/operations/list_pipeline_ios_devices)
operation.

## 3. Synchronize when needed

If the result says only **Device registered**, select **Sync Profiles** and
note the warning count. A successful automatic path reports
**Device registered and profiles synced**.

## Verify the result

The device appears under **Registered iOS Devices** with its name, normalized
UDID, and Apple status. The signing summary contains an updated profile for
each bundle identifier that the device must install.

## Troubleshooting

**The UDID is rejected**

Copy the complete identifier from Finder or Xcode. Remove spaces or punctuation
that are not part of the UDID.

**App Store Connect rejects the registration**

Confirm the stored API key is active and can manage devices for the configured
Apple team.

**Registration succeeds but profile sync reports warnings**

Run **Sync Profiles** again after confirming the bundle identifiers,
certificate, and profile access. Do not use an older profile that omits the new
device.

## Next step

Run a new signed build, then [install it on the registered
iPhone](/build/distribute/install/ios).
