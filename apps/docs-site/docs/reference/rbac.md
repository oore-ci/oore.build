---
status: implemented
description: "Role-based access control model with owner, admin, developer, and viewer roles."
---

# Roles and Permissions (RBAC)

oore.build uses role-based access control with four roles. Each user has exactly one role.

## Roles

| Role | Description |
|---|---|
| `owner` | Instance creator. Full access to everything. Exactly one per instance. |
| `admin` | Full management access. Can manage users, settings, and all project operations. |
| `developer` | Can create and manage projects, configure pipelines, trigger builds, and download artifacts. |
| `qa_viewer` | Read-only access. Can view builds and download artifacts but cannot modify anything. |

## Permission matrix

| Permission | Owner | Admin | Developer | QA Viewer |
|---|---|---|---|---|
| **Users** | | | | |
| View user list | Yes | Yes | No | No |
| Invite users | Yes | Yes | No | No |
| Change user roles | Yes | Yes | No | No |
| Disable/enable users | Yes | Yes | No | No |
| **Settings** | | | | |
| View instance settings | Yes | Yes | No | No |
| Modify artifact storage | Yes | Yes | No | No |
| Modify instance preferences | Yes | Yes | No | No |
| Manage integrations | Yes | Yes | No | No |
| **Projects** | | | | |
| List projects | Yes | Yes | Yes | Yes |
| Create projects | Yes | Yes | Yes | No |
| Edit projects | Yes | Yes | Yes | No |
| Delete projects | Yes | Yes | No | No |
| **Pipelines** | | | | |
| List pipelines | Yes | Yes | Yes | Yes |
| Create/edit pipelines | Yes | Yes | Yes | No |
| Delete pipelines | Yes | Yes | No | No |
| Configure signing | Yes | Yes | Yes | No |
| **Builds** | | | | |
| View builds | Yes | Yes | Yes | Yes |
| Trigger builds | Yes | Yes | Yes | No |
| Cancel builds | Yes | Yes | Yes | No |
| View build logs | Yes | Yes | Yes | Yes |
| **Artifacts** | | | | |
| List artifacts | Yes | Yes | Yes | Yes |
| Download artifacts | Yes | Yes | Yes | Yes |
| **Runners** | | | | |
| View runners | Yes | Yes | Yes | No |
| Register runners | Yes | Yes | No | No |

## User statuses

| Status | Description |
|---|---|
| `active` | User has signed in at least once and can access the system |
| `invited` | User has been invited but hasn't signed in yet |
| `disabled` | User account has been deactivated by an admin/owner |

## Role assignment rules

- The **owner** role is assigned during setup and cannot be changed via the API
- **Admins** can change roles for developers and QA viewers
- **Owners** can change any user's role (except their own)
- There is exactly one owner per instance
- New users are assigned a role at invitation time

## Enforcement

RBAC is enforced at the API level. Every authenticated request is checked against the user's role before processing. Insufficient permissions return `403 Forbidden` with code `forbidden`.

## Audit events

Role and user changes are logged:

| Event | Triggered when |
|---|---|
| `user_invited` | A new user is invited |
| `role_changed` | A user's role is modified |
| `user_disabled` | A user is disabled |
| `user_enabled` | A disabled user is re-enabled |
| `user_activated` | An invited user signs in for the first time |
| `owner_created` | The owner account is created during setup |
