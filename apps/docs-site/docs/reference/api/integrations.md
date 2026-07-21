---
status: implemented
description: 'API endpoints for GitHub and GitLab integrations in Oore CI.'
---

# Integrations API

Endpoints for managing source control integrations (GitHub and GitLab). All endpoints require a valid user session.

## List Integrations {#list-integrations}

```
GET /v1/integrations
```

**Authentication**: User session (Bearer)

### Response `200 OK`

```json
[
  {
    "id": "int_abc123",
    "provider": "github",
    "host_url": "https://github.com",
    "auth_mode": "github_app",
    "status": "active",
    "display_name": "My GitHub App",
    "app_id": "123456",
    "app_slug": "oore-build-ci",
    "created_by": "user_def456",
    "created_at": 1738800000,
    "updated_at": 1738800000
  }
]
```

---

## Get Integration {#get-integration}

```
GET /v1/integrations/{id}
```

**Authentication**: User session (Bearer)

---

## Delete Integration {#delete-integration}

```
DELETE /v1/integrations/{id}
```

**Authentication**: User session (Bearer)

---

## List Repositories {#list-repositories}

List repositories accessible through an integration.

```
GET /v1/integrations/{id}/repositories
```

**Authentication**: User session (Bearer)

### Response `200 OK`

Returns an array of repository objects with name, URL, and visibility.

Repository discovery does not grant execution independently. An Owner or Admin
trusts a repository when they link it to a project; there is no separate runner
policy endpoint or per-repository execution flag.

---

## GitHub Integration

### Start GitHub App Creation {#github-start}

```
POST /v1/integrations/github/start
```

Initiates the GitHub App creation flow. Returns a URL to redirect the user to GitHub.

### Complete GitHub App Setup {#github-complete}

```
POST /v1/integrations/github/complete
```

Completes the GitHub App OAuth flow after GitHub redirects back.

### GitHub Callback (internal) {#github-callback}

```
GET /v1/integrations/github/callback
```

Handles the OAuth callback from GitHub. This endpoint is called by GitHub, not directly by the UI.

### GitHub Installed (internal) {#github-installed}

```
GET /v1/integrations/github/installed
```

Handles the post-installation callback from GitHub.

### GitHub Create Page (internal) {#github-create}

```
GET /v1/integrations/github/create
```

Serves the GitHub App creation page.

### List Installations {#list-installations}

```
GET /v1/integrations/{id}/installations
```

List GitHub App installations for this integration.

### Sync Installations {#sync-installations}

```
POST /v1/integrations/{id}/installations
```

Force sync installation data from GitHub.

---

## GitLab Integration

### Start GitLab OAuth {#gitlab-start}

```
POST /v1/integrations/gitlab/start
```

Initiates the GitLab OAuth flow. Provide the GitLab host URL and OAuth credentials.

### Authorize GitLab {#gitlab-authorize}

```
POST /v1/integrations/gitlab/authorize
```

Completes the GitLab OAuth authorization.

### Rotate GitLab project webhook token {#gitlab-webhook-token}

```
POST /v1/integration-repositories/{id}/gitlab-webhook-secret
```

Generates or rotates the one-time-revealed webhook token for one synced GitLab project. The previous token for that project is invalid immediately.

### GitLab Callback (internal) {#gitlab-callback}

```
GET /v1/integrations/gitlab/callback
```

Handles the OAuth callback from GitLab.

---

## Webhooks

Webhook endpoints are outside the CORS-protected API group. They are called by external services.

### GitHub Webhook {#github-webhook}

```
POST /v1/webhooks/github
```

**Authentication**: GitHub webhook signature verification

Receives push and pull request events from GitHub. Pushes and verified same-repository PR open/reopen/new-revision actions can trigger matching pipelines. Forked, ambiguous, mismatched-target, and non-revision events are ignored.

### GitLab Webhook {#gitlab-webhook}

```
POST /v1/webhooks/gitlab
```

**Authentication**: repository-scoped GitLab webhook token plus matching payload `project.id`

Receives push and merge request events from GitLab. Pushes and verified same-project MR revisions can trigger matching pipelines. Forked, ambiguous, and non-revision events are ignored.
