---
status: implemented
---

# Auth0 OIDC Setup

This guide walks you through creating an Auth0 application and connecting it to oore.build.

## What you need

- An [Auth0](https://auth0.com/) account
- Permission to create applications in your Auth0 tenant
- Your oore.build instance ready for setup

## 1. Create an application

1. Sign in to the [Auth0 Dashboard](https://manage.auth0.com/)
2. Go to **Applications > Applications**
3. Click **Create Application**
4. Set:
   - **Name**: `oore.build`
   - **Application type**: Regular Web Applications
5. Click **Create**

## 2. Configure the application

Go to the **Settings** tab and set:

| Field | Value |
|---|---|
| **Allowed Callback URLs** | `http://127.0.0.1:4173/auth/callback, http://localhost:3000/auth/callback, https://ci.oore.build/auth/callback` |
| **Allowed Logout URLs** | `http://127.0.0.1:4173, http://localhost:3000` |
| **Allowed Web Origins** | `http://127.0.0.1:4173, http://localhost:3000` |

Both setup and regular sign-in use the same `/auth/callback` path — you only need one URI per origin. For a custom domain, replace the hosted UI URI with yours.

Click **Save Changes**.

## 3. Copy credentials

From the **Settings** tab, copy:

- **Domain** (e.g., `your-tenant.auth0.com`)
- **Client ID**
- **Client Secret**

## 4. Determine the issuer URL

Your Auth0 issuer URL is:

```
https://{your-domain}/
```

For example: `https://your-tenant.auth0.com/`

::: warning
Auth0 issuer URLs include a trailing slash. Make sure to include it.
:::

Verify discovery:

```bash
curl https://your-tenant.auth0.com/.well-known/openid-configuration | jq .issuer
```

## 5. Enter credentials in oore.build

During setup, enter:

| Field | Value |
|---|---|
| **Issuer URL** | `https://{your-domain}/` (include trailing slash) |
| **Client ID** | From step 3 |
| **Client secret** | From step 3 |

## Troubleshooting

### "OIDC discovery failed"

Verify the issuer URL includes the trailing slash and uses your Auth0 custom domain if configured.

### User can't sign in

Ensure the user exists in your Auth0 tenant and their email matches the invitation in oore.build.

## Reference

- [Auth0 Regular Web App Quickstart](https://auth0.com/docs/quickstart/webapp)
- [Auth0 OpenID Connect](https://auth0.com/docs/authenticate/protocols/openid-connect-protocol)
