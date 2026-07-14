---
status: implemented
description: 'Upload iOS signing certificates and provisioning profiles to Oore CI.'
---

# Acquire iOS Certificates and Profiles

Before you can sign iOS builds in Oore CI, you need signing certificates and provisioning profiles from Apple. This guide explains how to obtain them.

## What you need

- An [Apple Developer Program](https://developer.apple.com/programs/) membership ($99/year)
- Access to [App Store Connect](https://appstoreconnect.apple.com/) and the [Apple Developer portal](https://developer.apple.com/account/)
- A Mac with Keychain Access (for certificate export)

## Signing concepts

iOS code signing requires two assets:

| Asset                                         | Purpose                                             | Where to get it                                           |
| --------------------------------------------- | --------------------------------------------------- | --------------------------------------------------------- |
| **Signing certificate** (`.p12`)              | Proves your identity as the developer               | Created in Apple Developer portal, exported from Keychain |
| **Provisioning profile** (`.mobileprovision`) | Links your certificate to specific apps and devices | Created in Apple Developer portal                         |

## 1. Create a signing certificate

1. Go to the [Apple Developer portal — Certificates](https://developer.apple.com/help/account/create-certificates/)
2. Click the **+** button to create a new certificate
3. Choose the certificate type:
   - **Apple Development** — for development/testing
   - **Apple Distribution** — for App Store or ad hoc distribution
4. Follow the instructions to create a Certificate Signing Request (CSR) using Keychain Access on your Mac
5. Upload the CSR and download the certificate
6. Double-click the downloaded `.cer` file to install it in your Keychain

### Export as .p12

Oore CI needs the certificate in `.p12` format (which includes the private key):

1. Open **Keychain Access** on your Mac
2. Find the certificate you just installed (under "My Certificates")
3. Right-click and select **Export**
4. Choose **Personal Information Exchange (.p12)** format
5. Set an export password (you'll need this in Oore CI)
6. Save the file

## 2. Create a provisioning profile

1. Go to the [Apple Developer portal — Profiles](https://developer.apple.com/help/account/manage-profiles/)
2. Click **+** to create a new profile
3. Choose the profile type:
   - **iOS App Development** — for development builds
   - **Ad Hoc** — for distributing to registered test devices
   - **App Store** — for App Store submission
4. Select the App ID for your application
5. Select the certificate you created in step 1
6. For Ad Hoc profiles, select the test devices
7. Name the profile and download it

## 3. Upload to Oore CI

See one of:

- [Manual signing](/guides/signing/ios-manual-signing) — upload certificate and profile files
- [API signing](/guides/signing/ios-api-signing) — use App Store Connect API for automatic management

## Reference

- [Creating certificates — Apple Developer](https://developer.apple.com/help/account/create-certificates/)
- [Managing provisioning profiles — Apple Developer](https://developer.apple.com/help/account/manage-profiles/)
- [Registering test devices — Apple Developer](https://developer.apple.com/help/account/register-devices/)
- [Flutter iOS deployment](https://docs.flutter.dev/deployment/ios)
