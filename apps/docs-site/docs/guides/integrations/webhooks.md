---
status: implemented
description: "Configure webhook events and payloads for oore.build integrations."
---

# Webhook Troubleshooting

Webhooks enable automatic build triggers from GitHub and GitLab. This guide covers common issues.

## How webhooks work

When you push code or open a pull request:

1. GitHub/GitLab sends a POST request to your oore.build instance
2. The daemon verifies the webhook signature/token
3. If the event matches a pipeline's trigger config, a build is created

## Webhook endpoints

| Provider | Endpoint |
|---|---|
| GitHub | `POST /v1/webhooks/github` |
| GitLab | `POST /v1/webhooks/gitlab` |

## GitHub webhook troubleshooting

### Webhooks are configured automatically

When you [connect a GitHub App](/guides/integrations/github-app), webhooks are registered automatically. You don't need to manually configure them.

### Webhook delivery failures

Check GitHub's webhook delivery log:

1. Go to your GitHub organization settings
2. Navigate to **Installed GitHub Apps > oore.build > Advanced**
3. Check the **Recent Deliveries** list for failures

Common causes:

| Error | Fix |
|---|---|
| Connection refused | Ensure the daemon is running and reachable from the internet |
| 401 Unauthorized | Verify the GitHub App credentials are valid |
| Timeout | Check network connectivity between GitHub and your instance |

### No builds triggered

Even if the webhook is delivered successfully, a build only starts if:

1. The event type matches the pipeline's `triggers.events` setting
2. The branch matches the pipeline's `triggers.branches` setting
3. The pipeline is enabled

## GitLab webhook troubleshooting

### Manual configuration required

Unlike GitHub, [GitLab webhooks](/guides/integrations/gitlab) must be configured manually in the GitLab project settings.

### Testing webhooks

In GitLab, go to **Project Settings > Webhooks** and click **Test** to send a test event.

### Signature verification

Ensure the webhook secret token in GitLab matches the token from your oore.build integration settings.

## General troubleshooting

### Daemon not reachable

Webhooks require your daemon to be reachable from the internet (or from your GitLab instance). For local development:

- Use a tunnel service (e.g., ngrok, Cloudflare Tunnel) to expose `127.0.0.1:8787`
- Or configure webhooks to point to your production URL

### Build created but stuck in queued

This is a runner issue, not a webhook issue. See [builds stuck in queued](/operations/troubleshooting#builds-stuck-in-queued).
