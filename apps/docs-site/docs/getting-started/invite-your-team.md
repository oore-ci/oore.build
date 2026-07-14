---
status: implemented
description: 'Invite team members to your Oore CI instance with role-based access control.'
---

# Invite Your Team

This tutorial walks you through inviting team members to your Oore CI instance and assigning them appropriate roles.

## What you need

- **Role**: owner or admin
- A running Oore CI instance in `ready` state
- Your team members' email addresses (must match their OIDC or trusted-proxy identity)

## 1. Open user management

1. Sign in to the web UI
2. Go to **Settings > Users**

## 2. Invite a user

1. Click **Invite User**
2. Enter the user's **email address** (must match the email from your configured auth mode)
3. Select a **role**:

   | Role          | What they can do                                                                   |
   | ------------- | ---------------------------------------------------------------------------------- |
   | **Admin**     | Manage users, projects, pipelines, settings — everything except ownership transfer |
   | **Developer** | Create projects, configure pipelines, trigger builds, download artifacts           |
   | **QA Viewer** | View builds and download artifacts (read-only access to projects)                  |

4. Click **Invite**

The user is created in `invited` status. They complete activation by signing in through the configured remote auth mode for the first time.

## 3. User activates their account

When the invited user:

1. Opens the web UI
2. Clicks **Sign in**
3. Authenticates using the same email they were invited with

Their account moves from `invited` to `active` status automatically.

## 4. Verify

Go to **Settings > Users** and confirm the user appears as `active` after their first sign-in.

## Roles overview

| Permission           | Owner | Admin | Developer | QA Viewer |
| -------------------- | ----- | ----- | --------- | --------- |
| Manage users         | Yes   | Yes   | No        | No        |
| Manage settings      | Yes   | Yes   | No        | No        |
| Create/edit projects | Yes   | Yes   | Yes       | No        |
| Configure pipelines  | Yes   | Yes   | Yes       | No        |
| Trigger builds       | Yes   | Yes   | Yes       | No        |
| View builds          | Yes   | Yes   | Yes       | Yes       |
| Download artifacts   | Yes   | Yes   | Yes       | Yes       |

For the full permission matrix, see [RBAC Reference](/reference/rbac).

## Next steps

- [Manage roles](/guides/users/manage-roles) — change user roles
- [Disable users](/guides/users/disable-users) — deactivate accounts
