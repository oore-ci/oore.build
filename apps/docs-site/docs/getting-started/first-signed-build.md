---
status: implemented
description: 'Configure code signing and build a signed APK or IPA with Oore CI.'
---

# Your First Signed Build

This tutorial walks you through adding Android signing to a pipeline and producing a signed APK.

## What you need

- A [project with a working build](/getting-started/first-build) (unsigned APK builds successfully)
- An Android keystore file (`.jks` or `.keystore`). If you don't have one, see [Generate an Android Keystore](/guides/signing/android-keystore).
- The keystore password, key alias, and key password

## 1. Generate a keystore (if needed)

If you already have a keystore, skip to step 2.

```bash
keytool -genkey -v -keystore my-release-key.jks \
  -keyalg RSA -keysize 2048 -validity 10000 \
  -alias my-key-alias
```

For detailed instructions, see the [Android keystore guide](/guides/signing/android-keystore).

## 2. Upload the keystore

1. Open your project in the web UI
2. Go to **Pipelines** and select your pipeline
3. Open the **Signing** tab
4. Under **Android Signing**, click **Configure**
5. Upload your `.jks` file
6. Enter the **keystore password**, **key alias**, and **key password**
7. Click **Save**

The keystore file and passwords are encrypted at rest.

## 3. Trigger a signed build

Trigger a new build (manual, webhook, or API). The runner automatically uses the uploaded signing configuration.

## 4. Verify

1. Open the completed build in the UI
2. Download the APK artifact
3. Verify it's signed:

```bash
apksigner verify --print-certs my-app-release.apk
```

You should see your certificate information in the output.

## What's next

- [Configure Gradle signing](/guides/signing/android-gradle) — advanced Gradle signing config
- [iOS signing](/guides/signing/ios-certificates) — sign iOS builds
- [Download artifacts](/guides/artifacts/download-artifacts) — manage build artifacts
