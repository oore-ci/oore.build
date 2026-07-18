---
status: implemented
description: 'Overview of code signing in Oore CI for Android and iOS builds.'
---

# Code Signing in Oore CI

This page explains why code signing exists, how Oore CI handles it, and the tradeoffs between signing modes.

## Why code signing matters

Mobile operating systems require apps to be cryptographically signed before they can be installed:

- **Android** — APKs must be signed with a keystore. The Play Store requires a consistent signing key across app updates.
- **iOS** — Apps must be signed with an Apple-issued certificate and paired with a provisioning profile that specifies which devices/distribution channels are allowed.

Without signing, builds produce unsigned artifacts that can't be installed on devices or submitted to app stores.

## Oore CI's approach

Oore CI stores signing credentials (keystores, certificates, profiles, API keys) encrypted at rest. The trusted runner parent:

1. Retrieves signing assets with an ephemeral job-scoped capability
2. Runs repository-controlled build stages without signing authority
3. Uses fixed runner-owned logic to sign the completed Android artifact or iOS archive
4. Verifies the signature, cleans private temporary state, and revokes access when the job leaves active execution

Signing credentials never leave your infrastructure — they're stored on the daemon (encrypted with AES-256-GCM) and only transmitted to the runner over your local network.

## Android signing

Android signing is straightforward: upload a keystore (`.jks` / `.keystore`), provide the passwords, and Oore CI handles the rest.

| What you provide          | What Oore CI does                                 |
| ------------------------- | ------------------------------------------------- |
| Keystore file + passwords | Stores encrypted and signs the completed artifact |

See [Generate an Android Keystore](/guides/signing/android-keystore) and [Configure Gradle Signing](/guides/signing/android-gradle).

## iOS signing modes

iOS signing is more complex because Apple requires both a certificate and a provisioning profile, and managing these assets involves the Apple Developer portal. Oore CI offers two modes:

### Manual mode

You obtain the certificate and profile yourself and upload them to Oore CI.

| Pros                               | Cons                                |
| ---------------------------------- | ----------------------------------- |
| Full control over signing assets   | Manual certificate/profile renewal  |
| No App Store Connect access needed | Must manually register test devices |
| Works with enterprise certificates |                                     |

### API mode

You provide an App Store Connect API key, and Oore CI manages certificates and profiles automatically.

| Pros                                             | Cons                                  |
| ------------------------------------------------ | ------------------------------------- |
| Automatic certificate/profile management         | Requires App Store Connect API access |
| Register test devices through Oore CI            | Limited to Apple's API capabilities   |
| Automatic profile regeneration on device changes |                                       |

See [iOS Manual Signing](/guides/signing/ios-manual-signing) and [iOS API Signing](/guides/signing/ios-api-signing).

## Security model

- All signing credentials are encrypted at rest with AES-256-GCM
- The encryption key is stored in the macOS Keychain (with a file-based fallback)
- Credentials are transmitted only over HTTPS, except literal-loopback runner connections
- Only the actively assigned runner with the per-job signing capability receives credentials
- Repository-controlled commands never receive signing files, passwords, capability tokens, or an unlocked signing keychain
- Credentials are not included in build logs
