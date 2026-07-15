---
status: implemented
description: 'Configure Gradle signing for Android builds in Oore CI pipelines.'
---

# Configure Gradle Signing

This guide covers advanced Gradle signing configuration for Android builds in Oore CI.

## What you need

- **Role**: developer, admin, or owner
- An [Android keystore](/guides/signing/android-keystore) uploaded to your pipeline
- A Flutter project with an `android/` directory

## How Oore CI signing works

When a pipeline has Android signing configured, the runner:

1. Retrieves the keystore file and credentials from the daemon at build time
2. Writes `android/app/oore-upload-keystore.jks` and a compatible
   `android/key.properties` inside the runner's temporary checkout
3. Exposes the same signing values through `OORE_ANDROID_*` environment variables
4. Executes the build command (e.g., `flutter build apk --release`)
5. Cleans up the keystore after the build completes

You don't need to modify the conventional Flutter `build.gradle` for basic
signing. Oore's generated files exist only in the temporary checkout and never
change or get pushed to your repository.

## Custom Gradle configuration

For an explicit CI/local boundary, or for custom signing such as multiple
flavors, select Oore's environment variables when `CI=true` and retain your
existing local configuration otherwise:

```groovy
android {
    signingConfigs {
        release {
            if (System.getenv("CI")?.toBoolean() && System.getenv("OORE_ANDROID_KEYSTORE_PATH")) {
                storeFile file(System.getenv("OORE_ANDROID_KEYSTORE_PATH"))
                storePassword System.getenv("OORE_ANDROID_KEYSTORE_PASSWORD")
                keyAlias System.getenv("OORE_ANDROID_KEY_ALIAS")
                keyPassword System.getenv("OORE_ANDROID_KEY_PASSWORD")
            } else {
                storeFile file(keystoreProperties["storeFile"])
                storePassword keystoreProperties["storePassword"]
                keyAlias keystoreProperties["keyAlias"]
                keyPassword keystoreProperties["keyPassword"]
            }
        }
    }
    buildTypes {
        release {
            signingConfig signingConfigs.release
        }
    }
}
```

The runner sets `CI=true` and these environment variables when Android signing
is configured for the pipeline. Oore still creates the conventional temporary
files so projects without this branch continue to work with zero configuration.

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
