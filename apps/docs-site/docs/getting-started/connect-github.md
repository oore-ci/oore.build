---
status: implemented
description: "Connect a GitHub App to oore.build for automatic webhook-triggered builds."
---

# Connect GitHub

This tutorial walks you through connecting a GitHub account to your oore.build instance so you can import repositories and trigger builds from pushes and pull requests.

## What you need

- A running oore.build instance in `ready` state ([Set Up Your Instance](/getting-started/first-instance))
- A [GitHub](https://github.com/) account with access to the repositories you want to build
- Permission to install GitHub Apps on your account or organization

## 1. Start the GitHub integration

1. Open the oore.build web UI at `https://ci.oore.build` (or your self-hosted UI)
2. Sign in with your OIDC provider
3. Navigate to **Settings** and select **Integrations**
4. Click **Connect GitHub**

oore.build uses the [GitHub App](https://docs.github.com/en/apps/creating-github-apps/) model for integration. This provides fine-grained repository access without sharing personal access tokens.

## 2. Install the GitHub App

The UI redirects you to GitHub to create and install a GitHub App scoped to your oore.build instance:

1. **Authorize the app** — GitHub shows you the permissions the app needs (repository read access, webhook events)
2. **Choose repositories** — Select "All repositories" or pick specific ones
3. **Install** — Click Install to complete the process

After installation, GitHub redirects you back to oore.build, which stores the integration credentials.

## 3. Import repositories

Once connected:

1. Go to **Projects** and click **New Project**
2. Select the GitHub integration from the source dropdown
3. Browse your repositories or search by name
4. Select a repository and click **Create Project**

The project is created with the repository linked. Webhooks are configured automatically.

## 4. Verify the connection

Trigger a test:

1. Open the project you just created
2. Click **Trigger Build** (manual trigger)
3. The build should move from `queued` to `running` within a few seconds

If the build stays in `queued`, check that the embedded runner is active:
- Navigate to **Settings > Runners** and verify a runner is online
- If using `OORED_RUNNER_MODE=external`, start a runner with `oore runner start`

## What happens behind the scenes

When you connect GitHub:

1. `POST /v1/integrations/github/start` — begins the GitHub App creation flow
2. GitHub redirects to `GET /v1/integrations/github/callback` — oore.build completes the OAuth handshake
3. `GET /v1/integrations/github/installed` — confirms the app installation
4. `POST /v1/integrations/{id}/installations` — syncs the installation and repository data

Webhook events (`push`, `pull_request`) are received at `POST /v1/webhooks/github` and automatically trigger builds for configured pipelines.

## Next step

[Create your first build](/getting-started/first-build)
