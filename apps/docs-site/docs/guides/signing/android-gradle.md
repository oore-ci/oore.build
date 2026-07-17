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

1. Reserves the encrypted signing profile with a job-scoped capability
2. Executes the repository build command without signing files, signing
   environment variables, or an unlocked signing keychain
3. Creates a private signer workspace outside the repository checkout
4. Signs and verifies the resulting APK or App Bundle with fixed runner-owned
   tooling
5. Removes the private signer workspace and zeroes in-memory Android secrets

Repository code cannot customize or invoke the managed signer. Keep the build
itself unsigned; Oore replaces the produced artifact only after signature
verification succeeds.

## Custom Gradle configuration

For an explicit CI/local boundary, keep local developer signing and disable
Gradle signing in CI:

```groovy
android {
    signingConfigs {
        release {
            if (!System.getenv("CI")?.toBoolean()) {
                storeFile file(keystoreProperties["storeFile"])
                storePassword keystoreProperties["storePassword"]
                keyAlias keystoreProperties["keyAlias"]
                keyPassword keystoreProperties["keyPassword"]
            }
        }
    }
    buildTypes {
        release {
            if (!System.getenv("CI")?.toBoolean()) {
                signingConfig signingConfigs.release
            }
        }
    }
}
```

The runner sets `CI=true` but deliberately removes all legacy
`OORE_ANDROID_*` signing variables from repository child processes. Custom
flavors must still produce one APK or AAB for the fixed post-build signer.

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
