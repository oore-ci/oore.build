---
status: implemented
description: "Configure OpenID Connect authentication for oore.build with any OIDC-compatible identity provider."
---

# Configure OIDC Authentication

oore.build supports OpenID Connect (OIDC) authentication and does not support local passwords.

In Remote mode, OIDC is the default (`remote_auth_mode=oidc`). For local-first onboarding and operator access, the daemon also supports loopback-only local login (no OIDC). If you run behind an identity-aware proxy, you can opt into `remote_auth_mode=trusted_proxy` instead of OIDC.

## What you need

- An oore.build instance that is either being set up or already running
- Admin access to an OIDC-compatible identity provider
- The ability to create an OAuth 2.0 / OIDC application in your provider

## How OIDC works in oore.build

During setup, you provide three values:

| Value | Example | Where to get it |
|---|---|---|
| **Issuer URL** | `https://accounts.google.com` | Your provider's OIDC documentation |
| **Client ID** | `123456.apps.googleusercontent.com` | Created when you register an OAuth app |
| **Client secret** | `GOCSPX-...` | Created with the OAuth app (optional for some providers) |

oore.build uses the issuer URL to discover endpoints automatically via the [OpenID Connect Discovery](https://openid.net/specs/openid-connect-discovery-1_0.html) protocol. It fetches `{issuer_url}/.well-known/openid-configuration` to find the authorization, token, and JWKS endpoints.

The client secret, if provided, is encrypted with AES-256-GCM before storage.

## Required OAuth scopes

oore.build requests these scopes during authentication:

- `openid` — required by the OIDC spec
- `email` — used to identify users
- `profile` — used for display names and avatars

## Redirect URIs to configure

When creating your OAuth application, add these redirect URIs:

| Context | Redirect URI |
|---|---|
| Hosted UI (ci.oore.build) | `https://ci.oore.build/auth/callback` |
| Local launcher (`oore-web`) | `http://127.0.0.1:4173/auth/callback` |
| Local dev UI | `http://localhost:3000/auth/callback` |
| Custom domain | `https://your-domain.com/auth/callback` |
| CLI loopback | `http://localhost:*` (dynamic port shown by CLI) |

::: tip
Both setup and regular sign-in use the same `/auth/callback` path. You only need one redirect URI per origin.

Some providers don't support wildcard ports. In that case, the CLI will display the exact `http://localhost:<port>` URI before opening the browser — add it to your allowed redirect URIs at that point.
:::

::: tip
The setup wizard displays the exact redirect URI to configure based on how you access the UI.
:::

## Provider guides

Follow the guide for your identity provider:

| Provider | Guide |
|---|---|
| Google Workspace / Cloud Identity | [Google OIDC setup](/guides/oidc/google) |
| Okta | [Okta OIDC setup](/guides/oidc/okta) |
| Azure AD / Entra ID | [Azure AD OIDC setup](/guides/oidc/azure-ad) |
| Auth0 | [Auth0 OIDC setup](/guides/oidc/auth0) |
| Keycloak | [Keycloak OIDC setup](/guides/oidc/keycloak) |

Any provider that supports [OpenID Connect Discovery](https://openid.net/specs/openid-connect-discovery-1_0.html) will work. If your provider isn't listed above, use the general configuration steps:

1. Create an OAuth 2.0 / OIDC application in your provider
2. Set the application type to "Web application"
3. Add the redirect URIs listed above
4. Enable the `openid`, `email`, and `profile` scopes
5. Copy the issuer URL, client ID, and client secret
6. Enter them during oore.build setup (see [Set Up Your Instance](/getting-started/first-instance))

## Verify OIDC discovery

You can test that your issuer URL is correct before running setup:

```bash
curl https://accounts.google.com/.well-known/openid-configuration | jq .issuer
```

The response should include an `issuer` field matching your issuer URL.
