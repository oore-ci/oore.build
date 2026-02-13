---
status: implemented
description: "Invite new users to your oore.build instance via email."
---

# Invite Team Members

Add new users to your oore.build instance by sending email invitations.

## What you need

- **Role**: owner or admin
- The user's email address (must match their OIDC provider account)

## Steps

### 1. Send an invitation

In the web UI, go to **Settings > Users** and click **Invite User**.

Enter the email address and select a role:

| Role | Description |
|---|---|
| **Admin** | Full management access (users, settings, projects) |
| **Developer** | Create and manage projects, trigger builds |
| **QA Viewer** | Read-only access to builds and artifacts |

Click **Invite**.

### 2. User activation

The invited user signs in through the OIDC provider. On first sign-in, their account activates automatically.

::: info
The email address in the invitation must exactly match the email returned by the OIDC provider's ID token. If there's a mismatch, the user will see a "user not found" error.
:::

### Via API

```bash
curl -X POST http://127.0.0.1:8787/v1/users/invite \
  -H "Authorization: Bearer <session_token>" \
  -H "Content-Type: application/json" \
  -d '{
    "email": "dev@example.com",
    "role": "developer"
  }'
```

### Verify

Check **Settings > Users** — the user appears with `invited` status until they sign in.

## API endpoints

| Method | Path | Description |
|---|---|---|
| `POST` | `/v1/users/invite` | Invite a new user |
| `GET` | `/v1/users` | List all users |
