---
status: implemented
description: 'Set up Okta OIDC authentication for Oore CI.'
---

# Okta OIDC Setup

This guide walks you through creating an Okta OIDC application and connecting it to Oore CI.

## What you need

- An [Okta](https://www.okta.com/) account with admin access
- Permission to create applications in your Okta organization
- Your Oore CI instance ready for setup

## 1. Create an OIDC application

1. Sign in to the [Okta Admin Console](https://login.okta.com/)
2. Go to **Applications > Applications**
3. Click **Create App Integration**
4. Select **OIDC - OpenID Connect** and **Web Application**
5. Click **Next**

## 2. Configure the application

| Field                      | Value                                                                                                               |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| **App integration name**   | `Oore CI`                                                                                                           |
| **Grant type**             | Authorization Code                                                                                                  |
| **Sign-in redirect URIs**  | `http://127.0.0.1:4173/auth/callback`, `http://localhost:3000/auth/callback`, `https://ci.oore.build/auth/callback` |
| **Sign-out redirect URIs** | `http://127.0.0.1:4173`, `http://localhost:3000`                                                                    |
| **Controlled access**      | Choose your access policy                                                                                           |

Both setup and regular sign-in use the same `/auth/callback` path — you only need one URI per origin. For a custom domain, replace the hosted UI URI with yours.

Click **Save**.

## 3. Copy credentials

From the application's **General** tab, copy:

- **Client ID**
- **Client secret**

## 4. Find the issuer URL

Your Okta issuer URL follows this pattern:

```
https://{your-okta-domain}/oauth2/default
```

For example: `https://dev-123456.okta.com/oauth2/default`

Verify it supports OIDC discovery:

```bash
curl https://dev-123456.okta.com/oauth2/default/.well-known/openid-configuration | jq .issuer
```

## 5. Enter credentials in Oore CI

During setup, enter:

| Field             | Value                                       |
| ----------------- | ------------------------------------------- |
| **Issuer URL**    | `https://{your-okta-domain}/oauth2/default` |
| **Client ID**     | From step 3                                 |
| **Client secret** | From step 3                                 |

## Troubleshooting

### "Redirect URI mismatch"

Ensure the redirect URIs in Okta exactly match what Oore CI sends. Check for trailing slashes and `http` vs `https`.

### "OIDC discovery failed"

Verify the issuer URL includes `/oauth2/default` (or your custom authorization server path).

## Reference

- [Okta OIDC guide](https://developer.okta.com/docs/guides/implement-grant-type/authcode/main/)
- [Okta application setup](https://developer.okta.com/docs/guides/sign-into-web-app-redirect/main/)
