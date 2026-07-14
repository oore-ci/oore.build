---
status: implemented
description: 'Install and configure a GitHub App integration for Oore CI repository access and webhooks.'
---

# Connect a GitHub App

This guide covers connecting a GitHub App integration to your Oore CI instance for repository access and webhook-triggered builds.

## What you need

- **Role**: owner or admin
- A running Oore CI instance in `ready` state
- A [GitHub](https://github.com/) account with permission to install GitHub Apps on your target organization or personal account

## Steps

### 1. Start the integration flow

In the web UI, go to **Settings > Integrations** and click **Connect GitHub**.

Oore CI creates a [GitHub App](https://docs.github.com/en/apps/creating-github-apps/) scoped to your instance. This model provides:

- Fine-grained repository permissions (no broad personal access tokens)
- Automatic webhook registration
- Installation-level access controls

### 2. Authorize and install on GitHub

1. GitHub prompts you to review the app permissions
2. Select which repositories the app can access ("All repositories" or specific ones)
3. Click **Install**

### 3. Confirm in Oore CI

After GitHub redirects back, Oore CI:

1. Completes the OAuth handshake
2. Stores the integration credentials (encrypted at rest)
3. Syncs installations and repository metadata

The integration appears in **Settings > Integrations** as connected.

### 4. Verify

Go to **Projects > New Project** and confirm your GitHub repositories appear in the source selection dropdown.

## Managing installations

After the initial setup, manage repository access from GitHub:

1. Go to your GitHub organization or account settings
2. Navigate to **Installed GitHub Apps**
3. Click on the Oore CI app
4. Modify repository access as needed

Oore CI syncs installation changes automatically via webhooks. You can also force a sync from **Settings > Integrations > Sync** in the UI.

## Removing the integration

1. Go to **Settings > Integrations**
2. Click the GitHub integration
3. Click **Delete**

This removes the integration from Oore CI. To also uninstall the GitHub App, go to your GitHub organization settings and uninstall it there.

## API endpoints

| Method   | Path                                  | Description                  |
| -------- | ------------------------------------- | ---------------------------- |
| `POST`   | `/v1/integrations/github/start`       | Begin GitHub App creation    |
| `POST`   | `/v1/integrations/github/complete`    | Complete the OAuth flow      |
| `GET`    | `/v1/integrations/{id}/repositories`  | List accessible repositories |
| `GET`    | `/v1/integrations/{id}/installations` | List installations           |
| `POST`   | `/v1/integrations/{id}/installations` | Sync installations           |
| `DELETE` | `/v1/integrations/{id}`               | Remove integration           |

## Reference

- [GitHub Apps documentation](https://docs.github.com/en/apps/creating-github-apps/)
- [GitHub App permissions](https://docs.github.com/en/rest/overview/permissions-required-for-github-apps)
