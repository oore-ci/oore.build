---
status: implemented
description: 'Create a new project in Oore CI and link it to a source repository.'
---

# Create a Project

A project in Oore CI represents a single application repository. Each project can have one or more pipelines that define how to build the app.

## What you need

- **Role**: any authenticated user (developer, admin, or owner)
- A connected [GitHub](/guides/integrations/github-app) or [GitLab](/guides/integrations/gitlab) integration
- At least one repository accessible through the integration

## Steps

### 1. Open the new project dialog

In the web UI, navigate to **Projects** and click **New Project**.

### 2. Select a source

Choose the integration (GitHub or GitLab) and browse or search for the repository.

### 3. Configure the project

| Field          | Required | Description                  |
| -------------- | -------- | ---------------------------- |
| **Name**       | Yes      | Display name for the project |
| **Repository** | Yes      | The linked source repository |

### 4. Create

Click **Create Project**. The project is created and ready for pipeline configuration.

## Via the API

```bash
curl -X POST http://127.0.0.1:8787/v1/projects \
  -H "Authorization: Bearer <session_token>" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "My Flutter App",
    "repository_url": "https://github.com/org/my-flutter-app",
    "integration_id": "<integration_id>"
  }'
```

## Verify

The project appears in the **Projects** list. Open it to configure a pipeline.

## Next steps

- [Write a pipeline config](/guides/projects/pipeline-config)
- [Configure a pipeline via UI](/guides/projects/pipeline-ui-fallback)
- [Trigger builds](/guides/projects/trigger-builds)
