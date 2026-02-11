---
status: implemented
---

# Configure a Pipeline via the UI

If your repository doesn't contain a `.oore.yaml` file, you can configure the pipeline entirely through the web UI. The UI configuration acts as a fallback — if a `.oore.yaml` file is later added to the repo, it takes precedence.

## What you need

- **Role**: any authenticated user
- A [project](/guides/projects/create-project) in oore.build

## Steps

### 1. Open the pipeline editor

1. Navigate to your project in the web UI
2. Go to the **Pipelines** tab
3. Click **New Pipeline** (or edit an existing one)

### 2. Set basic configuration

| Field | Description |
|---|---|
| **Name** | A label for this pipeline (e.g., "Android Release", "iOS Ad Hoc") |
| **Platforms** | Select target platforms: Android, iOS, macOS |
| **Flutter version** | Optional fallback version (overridden by `.fvmrc` in the repo) |
| **Enabled** | Toggle the pipeline on/off |

### 3. Configure build commands

Set the commands for each stage:

| Stage | Description | Example |
|---|---|---|
| **Pre-build** | Runs before the main build | `flutter pub get` |
| **Build** | Main build commands | `flutter build apk --release` |
| **Post-build** | Runs after a successful build | (optional) |

### 4. Set artifact patterns

Define glob patterns to collect build artifacts:

```
**/*.apk
**/*.ipa
```

### 5. Configure triggers

| Field | Description |
|---|---|
| **Events** | Which events trigger builds: `push`, `pull_request` |
| **Branches** | Which branches trigger builds (supports glob patterns like `release/*`) |

### 6. Save

Click **Save Pipeline**. The configuration is stored and used when no `.oore.yaml` file is found in the repository at build time.

## How fallback resolution works

When a build is triggered:

1. oore.build checks the repository root for `.oore.yaml` (or `.oore.yml`)
2. If found, the file config is used — the UI config is ignored
3. If not found, the UI fallback config is used

This means you can start with UI configuration and migrate to file-based config later without breaking anything.

## API endpoints

| Method | Path | Description |
|---|---|---|
| `POST` | `/v1/projects/{project_id}/pipelines` | Create a pipeline |
| `PATCH` | `/v1/pipelines/{pipeline_id}` | Update pipeline config |
| `GET` | `/v1/pipelines/{pipeline_id}` | Get pipeline details |
| `DELETE` | `/v1/pipelines/{pipeline_id}` | Delete a pipeline |
