---
status: implemented
---

# Azure AD / Entra ID OIDC Setup

This guide walks you through registering an application in Microsoft Entra ID (formerly Azure AD) and connecting it to oore.build.

## What you need

- A [Microsoft Azure](https://portal.azure.com/) account
- Permission to register applications in your Entra ID tenant
- Your oore.build instance ready for setup

## 1. Register an application

1. Go to the [Azure Portal — App registrations](https://portal.azure.com/#blade/Microsoft_AAD_RegisteredApps/ApplicationsListBlade)
2. Click **New registration**
3. Set:
   - **Name**: `oore.build`
   - **Supported account types**: Choose based on who should access your instance
     - "Accounts in this organizational directory only" for single-tenant
     - "Accounts in any organizational directory" for multi-tenant
   - **Redirect URI**: Select "Web" and enter `http://127.0.0.1:4173/auth/callback`
4. Click **Register**

For detailed instructions, see [Register an application — Microsoft Entra](https://learn.microsoft.com/en-us/entra/identity-platform/quickstart-register-app).

## 2. Add additional redirect URIs

1. Go to **Authentication** in the app registration
2. Under **Web > Redirect URIs**, add:
   - `http://localhost:3000/auth/callback` (dev mode)
   - `https://ci.oore.build/auth/callback` (or your custom domain)
3. Click **Save**

Both setup and regular sign-in use the same `/auth/callback` path — you only need one URI per origin.

## 3. Create a client secret

1. Go to **Certificates & secrets**
2. Under **Client secrets**, click **New client secret**
3. Set a description and expiry period
4. Click **Add**
5. Copy the **Value** immediately (it's only shown once)

## 4. Find the issuer URL

Your Entra ID issuer URL follows this pattern:

```
https://login.microsoftonline.com/{tenant-id}/v2.0
```

Find your tenant ID on the **Overview** page of the app registration.

Verify discovery:

```bash
curl "https://login.microsoftonline.com/{tenant-id}/v2.0/.well-known/openid-configuration" | jq .issuer
```

## 5. Find the client ID

The **Application (client) ID** is on the **Overview** page of the app registration.

## 6. Enter credentials in oore.build

During setup, enter:

| Field | Value |
|---|---|
| **Issuer URL** | `https://login.microsoftonline.com/{tenant-id}/v2.0` |
| **Client ID** | Application (client) ID from step 5 |
| **Client secret** | Client secret value from step 3 |

## Troubleshooting

### "OIDC discovery failed"

Make sure the issuer URL includes `/v2.0`. The v1.0 endpoint has a different token format.

### "ID token missing email claim"

Ensure your app registration requests the `email` scope and that users have email addresses in your tenant.

## Reference

- [Register an application — Microsoft Entra](https://learn.microsoft.com/en-us/entra/identity-platform/quickstart-register-app)
- [Microsoft identity platform and OpenID Connect](https://learn.microsoft.com/en-us/entra/identity-platform/v2-protocols-oidc)
