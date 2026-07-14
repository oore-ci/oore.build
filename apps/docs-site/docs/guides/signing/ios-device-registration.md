---
status: implemented
description: 'Register iOS test devices for ad hoc distribution with Oore CI.'
---

# Register iOS Test Devices

For ad hoc distribution, you must register each test device's UDID with Apple. This guide covers registering devices through Oore CI.

## What you need

- **Role**: developer, admin, or owner
- A pipeline with [iOS API signing](/guides/signing/ios-api-signing) configured (API mode required for device registration through Oore CI)
- The test device's **UDID** (see below for how to find it)

## Find a device UDID

### From Finder (macOS)

1. Connect the iOS device to your Mac via USB
2. Open **Finder** and select the device in the sidebar
3. Click the device info area below the device name until the **UDID** appears
4. Right-click the UDID and select **Copy**

### From Xcode

1. Connect the device and open **Xcode**
2. Go to **Window > Devices and Simulators**
3. Select the device — the **Identifier** field is the UDID

## Register through Oore CI

1. Open your project in the web UI
2. Go to **Pipelines** and select the pipeline with iOS signing
3. Open the **Signing** tab
4. Under **Devices**, click **Register Device**
5. Enter the device **name** and **UDID**
6. Click **Register**

Oore CI registers the device with Apple via the App Store Connect API and updates the provisioning profile to include the new device.

## Register via API

```bash
curl -X POST http://127.0.0.1:8787/v1/pipelines/{pipeline_id}/ios-signing/devices/register \
  -H "Authorization: Bearer <session_token>" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Test iPhone 15",
    "udid": "00008110-000A1234567890AB"
  }'
```

## List registered devices

```bash
curl http://127.0.0.1:8787/v1/pipelines/{pipeline_id}/ios-signing/devices \
  -H "Authorization: Bearer <session_token>"
```

## After registration

After registering a new device, you need to regenerate the provisioning profile to include it. Click **Sync** in the signing configuration to refresh profiles from App Store Connect.

## Reference

- [Registering devices — Apple Developer](https://developer.apple.com/help/account/register-devices/)
- [Pipelines API — iOS Devices](/reference/api/pipelines#list-ios-devices)
