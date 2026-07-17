---
status: implemented
description: 'Configure manual iOS code signing with certificates and provisioning profiles.'
---

# iOS Manual Signing

Upload your signing certificate and provisioning profile directly to Oore CI for iOS builds.

## What you need

- **Role**: developer, admin, or owner
- A `.p12` signing certificate and its export password (see [Acquire iOS Certificates](/guides/signing/ios-certificates))
- A `.mobileprovision` provisioning profile (see [Acquire iOS Certificates](/guides/signing/ios-certificates))
- A [pipeline](/guides/projects/pipeline-config) configured for iOS builds
- A macOS runner started with `oore runner install-service` in an interactive login session

::: warning Runner session
iOS signing cannot run inside the `oored` system LaunchDaemon. Register the Mac as a Direct macOS runner and install its user service. The user may connect over SSH, but an interactive macOS login session must remain active for Apple Keychain code signing.
:::

## Steps

### 1. Open the signing configuration

1. Open your project in the web UI
2. Go to **Pipelines** and select the pipeline
3. Open the **Signing** tab
4. Under **iOS Signing**, select **Manual** mode

### 2. Upload the certificate

1. Click **Upload Certificate**
2. Select your `.p12` file
3. Enter the **certificate password** (the export password from Keychain Access)

### 3. Upload the provisioning profile

1. Click **Upload Profile**
2. Select your `.mobileprovision` file

### 4. Save

Click **Save**. The certificate and profile are encrypted at rest.

### 5. Verify

Trigger a build. The runner installs the certificate and profile before executing `flutter build ipa`. Download the resulting `.ipa` and verify:

```bash
codesign -dv --verbose=4 Payload/Runner.app
```

## When to use manual signing

Manual signing is appropriate when:

- You manage a small number of certificates and profiles
- You don't want to grant Oore CI access to App Store Connect
- You're using enterprise distribution certificates

For automatic certificate and profile management, see [API Signing](/guides/signing/ios-api-signing).

## API endpoint

```
PUT /v1/pipelines/{pipeline_id}/ios-signing
```

See [Pipelines API — iOS Signing](/reference/api/pipelines#update-ios-signing).

## Reference

- [Creating certificates — Apple Developer](https://developer.apple.com/help/account/create-certificates/)
- [Managing provisioning profiles — Apple Developer](https://developer.apple.com/help/account/manage-profiles/)
- [Flutter iOS deployment](https://docs.flutter.dev/deployment/ios)
