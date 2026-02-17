---
status: implemented
description: "Set up Keycloak OIDC authentication for Oore CI."
---

# Keycloak OIDC Setup

This guide walks you through creating a Keycloak client and connecting it to Oore CI.

## What you need

- A [Keycloak](https://www.keycloak.org/) instance with admin access
- A realm configured for your organization
- Your Oore CI instance ready for setup

## 1. Create a client

1. Sign in to the Keycloak Admin Console
2. Select your realm
3. Go to **Clients** and click **Create client**
4. Set:
   - **Client type**: OpenID Connect
   - **Client ID**: `oore-build`
5. Click **Next**

## 2. Configure the client

On the **Capability config** step:

- **Client authentication**: On (confidential client)
- **Authorization**: Off
- **Authentication flow**: Standard flow (check), Direct access grants (uncheck)

Click **Next**.

On the **Login settings** step:

| Field | Value |
|---|---|
| **Root URL** | `http://127.0.0.1:4173` |
| **Valid redirect URIs** | `http://127.0.0.1:4173/auth/callback`, `http://localhost:3000/auth/callback`, `https://ci.oore.build/auth/callback` |
| **Valid post logout redirect URIs** | `http://127.0.0.1:4173`, `http://localhost:3000` |
| **Web origins** | `http://127.0.0.1:4173`, `http://localhost:3000` |

Both setup and regular sign-in use the same `/auth/callback` path — you only need one URI per origin. For a custom domain, replace the hosted UI URI with yours.

Click **Save**.

## 3. Copy the client secret

1. Go to the **Credentials** tab of the client
2. Copy the **Client secret**

## 4. Find the issuer URL

Your Keycloak issuer URL follows this pattern:

```
https://{keycloak-host}/realms/{realm-name}
```

For example: `https://auth.mycompany.com/realms/main`

Verify discovery:

```bash
curl https://auth.mycompany.com/realms/main/.well-known/openid-configuration | jq .issuer
```

## 5. Enter credentials in Oore CI

During setup, enter:

| Field | Value |
|---|---|
| **Issuer URL** | `https://{keycloak-host}/realms/{realm-name}` |
| **Client ID** | `oore-build` (or whatever you chose in step 1) |
| **Client secret** | From step 3 |

## Troubleshooting

### "OIDC discovery failed"

Verify the issuer URL includes the full realm path (`/realms/{name}`). The base Keycloak URL alone won't work.

### "ID token missing email claim"

Ensure the Keycloak client has the `email` scope enabled and users have email addresses configured in your realm.

## Reference

- [Keycloak documentation](https://www.keycloak.org/documentation)
- [Keycloak OpenID Connect](https://www.keycloak.org/docs/latest/server_admin/#_oidc_clients)
