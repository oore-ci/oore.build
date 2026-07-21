---
status: implemented
description: 'Create a new project in Oore CI and link it to a source repository.'
---

# Create a Project

A project in Oore CI represents a single application repository. Each project can have one or more pipelines that define how to build the app.

## What you need

- **Role**: owner or admin
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

Click **Create Project**. Linking the repository is the Direct runner trust
decision: its checkout, dependencies, and build scripts may now run with the
runner account's permissions. There is no separate repository approval step.
The project is then ready for pipeline configuration.

## Choose or repair a project source

An Owner or Admin can open **Project > Settings** and choose **Source
repository** to link an unlinked project or change its source. Saving that field
is the same Direct runner trust decision as creating the project. Project
Maintainers can continue to edit the project name, description, and default
branch, but cannot change its source.

Changing the source cancels builds that are still queued or only scheduled so a
snapshot from the old repository cannot run under the new link. Assigned or
running work is allowed to finish with its original build-bound source. Trigger
a new build after the new source is saved.

An upgrade from the older per-repository execution gate may leave a project
unlinked when its repository was never approved. Its unassigned builds are canceled;
choose the source here before starting a new build. Previously approved links
remain intact.

## Via the API

```bash
curl -X POST http://127.0.0.1:8787/v1/projects \
  -H "Authorization: Bearer <session_token>" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "My Flutter App",
    "repository_id": "<repository_id>"
  }'
```

## Verify

The project appears in the **Projects** list. Open it to configure a pipeline.

## Next steps

- [Write a pipeline config](/guides/projects/pipeline-config)
- [Configure a pipeline via UI](/guides/projects/pipeline-ui-fallback)
- [Trigger builds](/guides/projects/trigger-builds)
