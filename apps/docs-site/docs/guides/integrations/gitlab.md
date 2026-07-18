---
status: implemented
description: 'Connect GitLab repositories to Oore CI for webhook-triggered builds.'
---

# Connect GitLab

This guide covers GitLab.com and self-managed GitLab sources, including private repository checkout and webhook-triggered builds.

## What you need

- **Role**: owner or admin
- A running Oore CI instance in `ready` state
- A [GitLab](https://about.gitlab.com/) account (self-hosted or gitlab.com)
- A GitLab personal access token, or permission to create an OAuth application

## Steps

## Choose an authentication method

Use a personal access token for the shortest setup, especially for an internal self-managed instance. Create the token with `read_user`, `read_api`, and `read_repository`. Oore encrypts the token and uses it for repository discovery and private checkout.

Use OAuth when your organization manages applications centrally. The Oore form shows the exact HTTPS callback URL to register. Select `read_api` and `read_repository`; full write-capable `api` access is not required.

## Connect GitLab

1. In the web UI, open **Sources** and choose **Connect GitLab**.
2. Enter the root origin, such as `https://gitlab.com` or `https://gitlab.example.com`. Do not append `/api/v4` or a group path.
3. Choose **Personal Access Token** or **OAuth Application** and follow the inline fields.
4. For OAuth, register the callback URL shown by Oore, save the source, then choose **Authorize on GitLab** from its details page.

Oore returns OAuth callbacks through the browser-facing Oore URL. In split deployments this is the AWS frontend/proxy URL, not the private macOS daemon address.

## Verify repository discovery

Open the source details page and choose **Sync GitLab projects**. Oore follows GitLab pagination, so project inventories larger than 100 are included. Repositories no longer visible to the GitLab account are removed from the source inventory.

Go to **Projects**, create a project, and confirm the repository picker identifies the GitLab host as well as the project path.

Before running builds, approve the repository in **Settings > Sources** and enable Direct macOS runner under **Settings > Runners**. Approval applies to every Oore project linked to that GitLab repository.

## Webhook configuration

When you create a project from a GitLab repository, Oore CI needs webhooks for automatic build triggers. Configure the webhook in your GitLab project:

1. In GitLab, go to **Project Settings > Webhooks**
2. In the Oore source details, generate a webhook token for this exact project. Copy it immediately; it is shown once.
3. Add a webhook using the values shown on the GitLab source screen:
   - **URL**: `https://<your-oore-frontend>/v1/webhooks/gitlab`
   - **Trigger**: Push events, Merge request events
   - **Secret token**: the token generated for this exact GitLab project
4. Click **Add webhook**

The URL must be reachable by GitLab. In a split deployment, the frontend proxy forwards this path to the private backend. A project token is accepted only when the payload's immutable GitLab project ID matches the repository that owns the token; a token from another project in the same integration is rejected. Rotating a token immediately invalidates the previous token. Oore derives a stable delivery identity when older/self-managed GitLab versions omit `X-Gitlab-Event-UUID`, so retries do not create duplicate builds.

Automatic merge-request builds are limited to verified same-project open/reopen events and updates that prove the head commit changed. Forked, ambiguous, label-only, closed, and merged events are ignored. External contribution approval is not available in V1.

## Private repository checkout

Runners fetch private GitLab repositories through an authenticated Oore checkout proxy. The stored GitLab token is decrypted only by the backend and is not written to build snapshots or logs.

Private submodules are not yet proxied. A build can clone public submodules normally, but a private GitLab submodule currently needs its own runner-visible credentials. Oore deliberately does not apply one integration token to every repository on the GitLab host.

OAuth access tokens are not refreshed automatically yet. If GitLab expires or revokes the token, re-authorize the source from its details page.

## Removing the integration

1. Go to **Settings > Integrations** in Oore CI
2. Open the GitLab source
3. Click **Disconnect**

Also revoke the OAuth application in GitLab if no longer needed.

## API endpoints

| Method   | Path                                                      | Description                                |
| -------- | --------------------------------------------------------- | ------------------------------------------ |
| `POST`   | `/v1/integrations/gitlab/start`                           | Begin GitLab OAuth flow                    |
| `POST`   | `/v1/integrations/gitlab/authorize`                       | Complete authorization                     |
| `GET`    | `/v1/integrations/{id}/repositories`                      | List accessible repositories               |
| `POST`   | `/v1/integration-repositories/{id}/gitlab-webhook-secret` | Generate or rotate a project webhook token |
| `PUT`    | `/v1/integration-repositories/{id}/runner-policy`         | Approve or revoke Direct runner execution  |
| `DELETE` | `/v1/integrations/{id}`                                   | Remove integration                         |

## Reference

- [GitLab OAuth 2.0 provider](https://docs.gitlab.com/api/oauth2/)
- [GitLab webhooks](https://docs.gitlab.com/ee/user/project/integrations/webhooks.html)
