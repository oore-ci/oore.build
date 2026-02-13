---
status: implemented
description: "Upload and manage Android keystores for signing builds in oore.build."
---

# Generate an Android Keystore

This guide covers generating an Android signing keystore and uploading it to oore.build.

## What you need

- **Role**: developer, admin, or owner
- Java `keytool` (included with JDK) or [Android Studio](https://developer.android.com/studio/publish/app-signing#generate-key)
- A [pipeline](/guides/projects/pipeline-config) configured for Android builds

## 1. Generate the keystore

Using `keytool` (included with any JDK installation):

```bash
keytool -genkey -v -keystore my-release-key.jks \
  -keyalg RSA -keysize 2048 -validity 10000 \
  -alias my-key-alias
```

You'll be prompted for:

| Prompt | Description |
|---|---|
| **Keystore password** | Password to protect the keystore file |
| **Key password** | Password for the specific key (can be same as keystore password) |
| **Name/organization fields** | Certificate identity information |

::: warning
Store your keystore file and passwords securely. If lost, you cannot update apps signed with this key on the Play Store. Google recommends using [Play App Signing](https://developer.android.com/studio/publish/app-signing#app-signing-google-play) for production apps.
:::

For the full Android documentation on keystores, see [Sign your app - Android Developers](https://developer.android.com/studio/publish/app-signing#generate-key).

## 2. Upload to oore.build

1. Open your project in the web UI
2. Go to **Pipelines** and select the pipeline
3. Open the **Signing** tab
4. Under **Android Signing**, click **Configure**
5. Upload the `.jks` file
6. Enter:
   - **Keystore password**
   - **Key alias** (e.g., `my-key-alias`)
   - **Key password**
7. Click **Save**

The keystore and passwords are encrypted at rest using AES-256-GCM.

## 3. Verify

Trigger a build. The runner uses the uploaded keystore to sign the APK. Download the artifact and verify:

```bash
apksigner verify --print-certs my-app-release.apk
```

## API endpoint

```
PUT /v1/pipelines/{pipeline_id}/android-signing
```

See [Pipelines API — Android Signing](/reference/api/pipelines#update-android-signing).

## Reference

- [Sign your app — Android Developers](https://developer.android.com/studio/publish/app-signing#generate-key)
- [Flutter Android deployment](https://docs.flutter.dev/deployment/android)
