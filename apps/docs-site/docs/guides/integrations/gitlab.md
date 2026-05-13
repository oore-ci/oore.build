---
status: implemented
description: "Connect GitLab repositories to Oore CI for webhook-triggered builds."
---

# Connect GitLab

This guide covers connecting a GitLab instance to Oore CI for repository access and webhook-triggered builds.

## What you need

- **Role**: owner or admin
- A running Oore CI instance in `ready` state
- A [GitLab](https://about.gitlab.com/) account (self-hosted or gitlab.com)
- Permission to create project webhooks in GitLab
- Either a Personal Access Token with `api` and `read_repository` scopes, or permission to create OAuth applications in your GitLab instance

## Personal Access Token mode

Use this mode for self-managed/internal GitLab instances when you want the quickest setup.

1. In GitLab, create a Personal Access Token with:
   - `api`: lets Oore list accessible projects and read integration metadata
   - `read_repository`: lets Oore runners clone private repositories over HTTPS
2. In Oore, go to **Settings > Integrations > Connect GitLab**.
3. Enter the GitLab host URL and token.
4. Copy the displayed webhook URL and generated webhook secret. You need both when configuring GitLab webhooks.

## OAuth application mode

### 1. Create a GitLab OAuth application

1. In GitLab, go to **Admin Area > Applications** (for instance-wide) or **User Settings > Applications** (for personal)
2. Create a new application:
   - **Name**: `Oore CI`
   - **Redirect URI**: `http://127.0.0.1:8787/v1/integrations/gitlab/callback`
   - **Scopes**: `api`, `read_repository`
   - **Confidential**: Yes
3. Save and copy the **Application ID** and **Secret**

For GitLab OAuth documentation, see [GitLab OAuth 2.0 provider](https://docs.gitlab.com/api/oauth2/).

### 2. Start the integration in Oore CI

1. In the web UI, go to **Settings > Integrations**
2. Click **Connect GitLab**
3. Enter your GitLab host URL (e.g., `https://gitlab.com` or your self-hosted URL)
4. Enter the Application ID and Secret from step 1

### 3. Authorize

Oore CI redirects you to GitLab to authorize the OAuth application. After authorization, GitLab redirects back and Oore CI stores the credentials.

### 4. Verify

Go to **Projects > New Project** and confirm your GitLab repositories appear in the source selection dropdown.

## Webhook configuration

When you create a project from a GitLab repository, Oore CI needs webhooks for automatic build triggers. Configure the webhook in your GitLab project:

1. In GitLab, go to **Project Settings > Webhooks**
2. Add a webhook:
   - **URL**: `<your public Oore URL>/v1/webhooks/gitlab`
   - **Trigger**: Push events
   - **Secret token**: use the webhook secret shown in Oore
3. Click **Add webhook**

## Removing the integration

1. Go to **Settings > Integrations** in Oore CI
2. Click the GitLab integration
3. Click **Delete**

Also revoke the OAuth application in GitLab if no longer needed.

## API endpoints

| Method | Path | Description |
|---|---|---|
| `POST` | `/v1/integrations/gitlab/start` | Begin GitLab OAuth flow |
| `POST` | `/v1/integrations/gitlab/authorize` | Complete authorization |
| `GET` | `/v1/integrations/{id}/repositories` | List accessible repositories |
| `DELETE` | `/v1/integrations/{id}` | Remove integration |

## Reference

- [GitLab OAuth 2.0 provider](https://docs.gitlab.com/api/oauth2/)
- [GitLab webhooks](https://docs.gitlab.com/ee/user/project/integrations/webhooks.html)
