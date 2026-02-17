---
status: implemented
description: "Set up Google Workspace OIDC authentication for Oore CI."
---

# Google OIDC Setup

This guide walks you through creating a Google OAuth 2.0 application and connecting it to Oore CI as your OIDC identity provider.

## What you need

- A [Google Cloud](https://console.cloud.google.com/) account with a project
- Permission to create OAuth 2.0 credentials in the Google Cloud Console
- Your Oore CI instance ready for setup (see [Set Up Your Instance](/getting-started/first-instance))

## 1. Create OAuth credentials

1. Go to the [Google Cloud Console — Credentials](https://console.cloud.google.com/apis/credentials) page.

2. Click **Create Credentials** and select **OAuth client ID**.

3. If prompted, configure the OAuth consent screen first:
   - Choose **Internal** (for Google Workspace users) or **External** (for any Google account)
   - Fill in the required app name and email fields
   - Add the scopes: `openid`, `email`, `profile`
   - Save and return to the Credentials page

4. For application type, select **Web application**.

5. Set a name (e.g., "Oore CI").

6. Under **Authorized redirect URIs**, add:

   ```
   http://127.0.0.1:4173/auth/callback
   http://localhost:3000/auth/callback
   https://ci.oore.build/auth/callback
   ```

   Both setup and regular sign-in use the same `/auth/callback` path, so you only need one URI per origin.

   ::: tip
   If you plan to use the CLI for setup, you'll also need to add the loopback URI shown by the CLI (e.g., `http://localhost:52341`). The CLI displays this before opening the browser.
   :::

7. Click **Create**.

8. Copy the **Client ID** and **Client Secret** from the dialog. You'll need these during Oore CI setup.

## 2. Note the issuer URL

Google's OIDC issuer URL is:

```
https://accounts.google.com
```

You can verify it supports discovery:

```bash
curl https://accounts.google.com/.well-known/openid-configuration | jq .issuer
# Expected: "https://accounts.google.com"
```

## 3. Enter credentials during Oore CI setup

During setup (either web UI or CLI), enter:

| Field | Value |
|---|---|
| **Issuer URL** | `https://accounts.google.com` |
| **Client ID** | The client ID from step 1 (e.g., `123456789.apps.googleusercontent.com`) |
| **Client secret** | The client secret from step 1 (e.g., `GOCSPX-...`) |

Oore CI will perform OIDC discovery against the issuer URL, verify the configuration, and store the credentials (the client secret is encrypted with AES-256-GCM).

## 4. Verify authentication

After setup completes:

1. Open the web UI at `http://127.0.0.1:4173` (or `http://localhost:3000` in dev mode)
2. Click **Sign in**
3. You should be redirected to Google's sign-in page
4. After authenticating, you should be redirected back to Oore CI and logged in

## Google Workspace considerations

If you chose **Internal** for the consent screen:

- Only users in your Google Workspace organization can sign in
- No app verification is required
- This is the recommended option for team use

If you chose **External**:

- Any Google account can sign in (subject to your Oore CI RBAC — uninvited users will be rejected)
- Google may require app verification for production use
- For internal/testing use, you can add test users in the consent screen settings

## Troubleshooting

### "OIDC discovery failed"

Verify the issuer URL is exactly `https://accounts.google.com` (no trailing slash). Test with:

```bash
curl -s https://accounts.google.com/.well-known/openid-configuration | jq .
```

### "Redirect URI mismatch"

Google is strict about redirect URI matching. Ensure the URI in your OAuth credentials exactly matches what Oore CI sends. Check for:

- `http` vs `https`
- Trailing slashes
- Port numbers

### "Access blocked: app has not completed the Google verification process"

This happens with **External** consent screens in production mode. For testing, add your Google account as a test user in the OAuth consent screen settings. For production use, submit the app for [Google verification](https://support.google.com/cloud/answer/9110914).

## Reference

- [Google OpenID Connect documentation](https://developers.google.com/identity/openid-connect/openid-connect)
- [Google Cloud Console — Credentials](https://console.cloud.google.com/apis/credentials)
- [OAuth consent screen configuration](https://console.cloud.google.com/apis/credentials/consent)
