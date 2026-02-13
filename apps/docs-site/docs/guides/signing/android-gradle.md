---
status: implemented
description: "Configure Gradle signing for Android builds in oore.build pipelines."
---

# Configure Gradle Signing

This guide covers advanced Gradle signing configuration for Android builds in oore.build.

## What you need

- **Role**: developer, admin, or owner
- An [Android keystore](/guides/signing/android-keystore) uploaded to your pipeline
- A Flutter project with an `android/` directory

## How oore.build signing works

When a pipeline has Android signing configured, the runner:

1. Retrieves the keystore file and credentials from the daemon at build time
2. Places the keystore in a temporary location on the runner
3. Sets the signing configuration so Gradle can find it
4. Executes the build command (e.g., `flutter build apk --release`)
5. Cleans up the keystore after the build completes

You don't need to modify your `build.gradle` for basic signing — oore.build handles it automatically.

## Custom Gradle configuration

If your project requires custom signing configuration (e.g., multiple flavors with different keystores), you can reference environment variables in your `build.gradle`:

```groovy
android {
    signingConfigs {
        release {
            storeFile file(System.getenv("OORE_KEYSTORE_PATH"))
            storePassword System.getenv("OORE_KEYSTORE_PASSWORD")
            keyAlias System.getenv("OORE_KEY_ALIAS")
            keyPassword System.getenv("OORE_KEY_PASSWORD")
        }
    }
    buildTypes {
        release {
            signingConfig signingConfigs.release
        }
    }
}
```

These environment variables are set by the runner when Android signing is configured for the pipeline.

## Verify

After a successful signed build:

```bash
# Check APK signature
apksigner verify --print-certs app-release.apk

# Check AAB signature
jarsigner -verify -verbose app-release.aab
```

## Reference

- [Android app signing](https://developer.android.com/studio/publish/app-signing)
- [Flutter Android deployment](https://docs.flutter.dev/deployment/android)
