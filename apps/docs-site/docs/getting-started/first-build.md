---
status: implemented
description: 'Trigger and monitor your first Flutter build on Oore CI.'
---

# Create Your First Build

This tutorial walks you through creating a project, configuring a pipeline, and triggering your first Flutter build.

## What you need

- A running Oore CI instance in `ready` state ([Set Up Your Instance](/getting-started/first-instance))
- A [GitHub](/getting-started/connect-github) or GitLab integration connected
- A Flutter project repository accessible through your integration
- An online Direct macOS runner (installed automatically with a local macOS backend)
- **Accept new builds** on in **Settings > Preferences**
- An Owner/Admin-created project whose linked source is available

## 1. Create a project

1. Open the web UI at `https://ci.oore.build` (or your self-hosted UI)
2. Sign in using the auth mode configured for your instance
3. Click **New Project**
4. Select your integration (GitHub or GitLab)
5. Choose the repository containing your Flutter app
6. Give the project a name and click **Create**

## 2. Configure a pipeline

After creating the project, set up a build pipeline:

### Option A: Use a `.oore.yaml` file (recommended)

Add a `.oore.yaml` file to the root of your repository:

```yaml
version: 1
flutter_version: '3.24.0'
platforms:
  - android
commands:
  pre_build:
    - flutter pub get
  build:
    - flutter build apk --release
  post_build: []
artifacts:
  patterns:
    - '**/*.apk'
```

Push the file to your repository. Oore CI reads this file at build time — no UI configuration needed.

If your repository includes a `.fvmrc` file, Oore CI uses that Flutter version
automatically. The `flutter_version` field is a fallback; otherwise Oore
downloads and caches stable Flutter automatically.

### Option B: Configure via the UI

If you don't want a config file in your repo:

1. Open the project in the web UI
2. Go to **Pipelines** and click **New Pipeline**
3. Set the pipeline name and select target platforms
4. Configure build commands:
   - **Pre-build**: `flutter pub get`
   - **Build**: `flutter build apk --release`
5. Set artifact patterns: `**/*.apk`
6. Click **Save**

The UI fallback configuration is used when no `.oore.yaml` file exists in the repository.

## 3. Trigger a build

You can trigger builds three ways:

### Manual trigger (UI)

1. Open the project
2. Click **Trigger Build**
3. Select the pipeline and branch
4. For a multi-platform pipeline, keep every platform selected or choose the platforms needed for this run
5. Click **Start Build**

The platform choice affects only that manual run. Automatic builds still run every platform configured by the pipeline, and a re-run keeps the original selection.

### Webhook trigger (automatic)

Push a commit or update a same-repository pull request. If your pipeline's trigger config matches the branch and event, a build starts automatically. External-fork pull requests are ignored in Direct runner mode.

### API trigger

```bash
curl -X POST http://127.0.0.1:8787/v1/projects/{project_id}/builds \
  -H "Authorization: Bearer <session_token>" \
  -H "Content-Type: application/json" \
  -d '{
    "pipeline_id": "<pipeline_id>",
    "platforms": ["android"],
    "branch": "main"
  }'
```

## 4. Watch the build

1. Open the project and click on the running build
2. The build log streams in real-time via Server-Sent Events
3. Watch as the build moves through states: `queued` → `assigned` → `running` → `succeeded`

When the build succeeds, artifacts (e.g., the `.apk` file) appear in the build details.

## Build states

| State       | Meaning                                           |
| ----------- | ------------------------------------------------- |
| `queued`    | Waiting for a runner to pick up the job           |
| `scheduled` | Assigned to a runner, waiting to start            |
| `assigned`  | Runner has claimed the job                        |
| `running`   | Build commands executing                          |
| `succeeded` | Build completed successfully                      |
| `failed`    | Build commands returned a non-zero exit code      |
| `canceled`  | Build was manually canceled                       |
| `timed_out` | Build exceeded the time limit                     |
| `expired`   | Build sat in queue too long without being claimed |

For the full state machine, see [Build States](/reference/build-states).

## Troubleshooting

### Build stuck in "queued"

- Read the waiting reason on the build page. Turn on **Accept new builds** in **Settings > Preferences**, or reconnect the project's source when prompted.
- Check **Settings > Runners** for an active runner.
- For a foreground diagnostic run, start it with `oore runner start`. Normal backend installations should use `oore runner install-service --managed-local`; a separately registered external runner uses `oore runner install-service`.

### Flutter commands fail

Run `oore doctor` to check your toolchain:

```bash
oore doctor
```

Oore bundles FVM and downloads the selected Flutter SDK automatically. If this
check fails, update or reinstall Oore. Android builds still require an Android
SDK, and Apple builds require full Xcode.

### "No .oore.yaml found" but UI pipeline exists

This is normal — the UI fallback config is used when no file exists. Builds will use the pipeline's execution config from the UI.

## Next steps

- [Configure pipeline YAML](/guides/projects/pipeline-config) — full `.oore.yaml` reference
- [Trigger builds](/guides/projects/trigger-builds) — all trigger methods
- [Add Android signing](/getting-started/first-signed-build) — sign your APKs (Wave 3)
