---
status: implemented
description: "Use App Store Connect API for automatic iOS signing in Oore CI."
---

# iOS App Store Connect API Signing

Use the App Store Connect API to let Oore CI automatically manage certificates and provisioning profiles.

## What you need

- **Role**: admin or owner
- An [Apple Developer Program](https://developer.apple.com/programs/) membership
- An [App Store Connect API key](https://developer.apple.com/documentation/appstoreconnectapi/creating_api_keys_for_app_store_connect_api) with appropriate permissions
- A [pipeline](/guides/projects/pipeline-config) configured for iOS builds

## 1. Create an API key

1. Go to [App Store Connect — Users and Access — Keys](https://appstoreconnect.apple.com/access/api)
2. Click **Generate API Key**
3. Set a name (e.g., "Oore CI signing")
4. Select the **Developer** or **Admin** role
5. Click **Generate**
6. Download the `.p8` key file (you can only download it once)
7. Note the **Key ID** and **Issuer ID** shown on the page

For detailed instructions, see [Creating API Keys for App Store Connect API](https://developer.apple.com/documentation/appstoreconnectapi/creating_api_keys_for_app_store_connect_api).

## 2. Configure in Oore CI

1. Open your project in the web UI
2. Go to **Pipelines** and select the pipeline
3. Open the **Signing** tab
4. Under **iOS Signing**, select **API** mode
5. Enter:
   - **Issuer ID** — from App Store Connect
   - **Key ID** — from App Store Connect
   - **API key file** — upload the `.p8` file
6. Click **Save**

## 3. Sync certificates and profiles

After configuring the API key:

1. Click **Sync** in the signing configuration
2. Oore CI connects to App Store Connect and fetches available certificates and profiles
3. Select the certificate and profile to use for this pipeline

This calls `POST /v1/pipelines/{pipeline_id}/ios-signing/sync`.

## 4. Verify

Trigger a build. The runner uses the synced certificate and profile for signing.

## When to use API signing

API signing is appropriate when:

- You want Oore CI to manage certificates and profiles automatically
- You have many pipelines or frequently rotate signing assets
- You want to register test devices through Oore CI

## API endpoints

| Method | Path | Description |
|---|---|---|
| `PUT` | `/v1/pipelines/{pipeline_id}/ios-signing` | Configure API signing |
| `POST` | `/v1/pipelines/{pipeline_id}/ios-signing/sync` | Sync from App Store Connect |
| `GET` | `/v1/pipelines/{pipeline_id}/ios-signing` | Get current config |

## Reference

- [Creating API Keys — App Store Connect](https://developer.apple.com/documentation/appstoreconnectapi/creating_api_keys_for_app_store_connect_api)
- [App Store Connect API](https://developer.apple.com/app-store-connect/api/)
