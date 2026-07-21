---
status: implemented
description: 'Instance and project role-based access control for owners, admins, developers, and QA viewers.'
---

# Roles and Permissions (RBAC)

Oore CI gives each user one instance role. Developers and QA viewers also need explicit project membership; owners and admins have implicit Maintainer access to every project.

## Roles

| Role        | Description                                                                                         |
| ----------- | --------------------------------------------------------------------------------------------------- |
| `owner`     | Instance creator. Full access to everything. Exactly one per instance.                              |
| `admin`     | Full management access. Can manage users, settings, and all project operations.                     |
| `developer` | Can use assigned operator surfaces and mutate an assigned project when its project role permits it. |
| `qa_viewer` | Tester-only access to assigned apps, release details, diagnostic logs, and installable artifacts.   |

## Operator surface policy

| Surface                         | Owner        | Admin        | Developer                                        | QA Viewer                    |
| ------------------------------- | ------------ | ------------ | ------------------------------------------------ | ---------------------------- |
| Operator dashboard              | Full         | Full         | Assigned work                                    | No; tester workspace instead |
| Create projects                 | Yes          | Yes          | No                                               | No                           |
| Existing project/build surfaces | All projects | All projects | Assigned projects; mutations follow project role | Assigned release data only   |
| Users and invitations           | Manage       | Manage       | No                                               | No                           |
| Sources                         | Manage       | Manage       | Read-only                                        | No operator route            |
| Runners                         | Manage       | Manage       | Read-only                                        | No operator route            |
| API tokens                      | Manage       | Manage       | Manage within developer visibility               | No                           |
| Notifications                   | Manage       | Manage       | No                                               | No                           |
| Preferences and retention       | Manage       | Manage       | No                                               | No                           |
| Audit log                       | View         | View         | No                                               | No                           |
| Runtime updates                 | Manage       | No           | No                                               | No                           |

Notification routes and all instance-administration routes are owner/admin-only unless the table explicitly narrows access further. Runtime update controls are owner-only.

## Project roles

| Project role | Capabilities                                                                                                                                                       |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `maintainer` | Full project access, including project settings and deletion, membership, pipelines and signing, builds, and artifacts.                                            |
| `developer`  | Read the project; manage pipelines and signing; trigger or cancel builds; read artifacts and manage artifact shares. Cannot change project settings or membership. |
| `viewer`     | Read project, pipeline, build, log, and artifact data; download or install artifacts. No project mutations.                                                        |

Owners and admins resolve to Maintainer for every project. A developer's effective permissions come from the role on that explicit membership. QA viewers are always capped at Viewer, including when legacy membership data contains a higher role.

Project Maintainers can discover only eligible developer and QA-viewer candidates through the project-scoped membership API. This does not grant developer accounts access to the global Users directory.

## Route behavior

- Developers can use the dashboard, assigned project and build routes, API tokens, and read-only Sources and Runners. Project mutations are checked against the effective project role before mutation UI renders. Only Owners and Admins can create a project or change its linked repository because that action trusts repository code to run.
- QA viewers use the canonical `/` tester workspace. `/builds` redirects to `/`; an assigned `/builds/:buildId` opens tester release detail with release information, install/download action, and secondary diagnostic logs.
- QA viewers do not receive the operator Projects, Sources, Runners, Users, Notifications, or instance-settings routes.
- Disallowed direct routes redirect before their mutation interface renders. The backend remains the security boundary.
- There is no QA-preview or impersonation endpoint. The hosted demo's role selector signs in as a demo persona and does not change a real user's role or session.

## User statuses

| Status     | Description                                                |
| ---------- | ---------------------------------------------------------- |
| `active`   | User has signed in at least once and can access the system |
| `invited`  | User has been invited but hasn't signed in yet             |
| `disabled` | User account has been deactivated by an admin/owner        |

## Role assignment rules

- The **owner** role is assigned during setup and cannot be changed via the API
- **Admins** can change roles for developers and QA viewers
- **Owners** can change any user's role (except their own)
- There is exactly one owner per instance
- New users are assigned a role at invitation time

## Enforcement

RBAC is enforced at the API level. Instance operations check the session role; project operations also resolve membership and the effective project role. A project hidden from the user returns `404 Not Found`; an allowed project with an insufficient role returns `403 Forbidden`. Frontend route guards and hidden controls mirror this policy but do not replace backend enforcement.

## Audit events

Role and user changes are logged:

| Event            | Triggered when                              |
| ---------------- | ------------------------------------------- |
| `user_invited`   | A new user is invited                       |
| `role_changed`   | A user's role is modified                   |
| `user_disabled`  | A user is disabled                          |
| `user_enabled`   | A disabled user is re-enabled               |
| `user_activated` | An invited user signs in for the first time |
| `owner_created`  | The owner account is created during setup   |
