---
status: implemented
description: 'How Oore CI uses .oore.yaml for repository-level pipeline configuration.'
---

# File-First Configuration

How Oore CI resolves pipeline configuration from `.oore.yaml` files and UI settings.

## The resolution policy

Oore CI uses a **file-first, UI-fallback** configuration model. When a build is created, the daemon captures a config snapshot with the policy `file_first_ui_fallback`:

1. **Primary**: Look for a `.oore.yaml` file in the repository at the configured path
2. **Fallback**: Use the pipeline's UI-configured execution settings

This means teams that check in an `.oore.yaml` file get reproducible, version-controlled builds. Teams that prefer UI configuration still work — the UI settings act as the fallback when no config file is found.

## Config snapshot

At build creation time, the daemon captures a **config snapshot** — a frozen copy of all configuration that the runner needs. This snapshot is immutable for the lifetime of the build, even if the pipeline settings or `.oore.yaml` file change afterward.

The snapshot includes:

| Field                      | Description                                                    |
| -------------------------- | -------------------------------------------------------------- |
| `snapshot_version`         | Schema version (currently `2`)                                 |
| `config_resolution_policy` | Always `file_first_ui_fallback`                                |
| `config_path`              | Path to config file (default: `.oore.yaml`)                    |
| `config_path_explicit`     | Whether the user explicitly set the path vs using the default  |
| `ui_execution_config`      | Pipeline execution settings from the UI                        |
| `artifact_patterns`        | Glob patterns for artifact collection (e.g., `*.apk`, `*.ipa`) |
| `trigger_type`             | How the build was triggered (`manual` or `webhook`)            |
| `commit_sha`               | The exact commit to build                                      |
| `branch`                   | The branch name                                                |
| `repo_url`                 | Repository clone URL                                           |
| `captured_at`              | Unix timestamp when the snapshot was taken                     |

## Why snapshots matter

Without snapshots, a build triggered at 2:00 PM could use different configuration than what existed at 2:00 PM if someone edits the pipeline settings at 2:01 PM while the build is still running. Snapshots eliminate this race condition — every build uses exactly the configuration that existed when it was created.

## Config path

By default, the daemon looks for `.oore.yaml` at the repository root. You can change this per-pipeline:

- **Default**: `.oore.yaml` (with `config_path_explicit: false`)
- **Custom**: Set an explicit path in the pipeline settings (sets `config_path_explicit: true`)

See the [.oore.yaml reference](/reference/config/oore-yaml) for the full schema.

## UI execution config

The UI fallback stores pipeline settings configured through the web interface:

- **Platforms**: Which platforms to build for (android, ios, macos)
- **Build arguments**: Custom arguments passed to the build tool
- **Environment variables**: Key-value pairs injected into the build environment
- **Artifact patterns**: Glob patterns for collecting build outputs

These settings are captured in the snapshot's `ui_execution_config` field and used when no `.oore.yaml` file is found in the repository.

## Trigger types and config resolution

| Trigger                          | Config source  | Notes                                                                                                                     |
| -------------------------------- | -------------- | ------------------------------------------------------------------------------------------------------------------------- |
| **Manual** (API or UI)           | UI config only | Snapshot contains UI settings; `.oore.yaml` is available to the runner for override                                       |
| **Webhook** (GitHub/GitLab push) | Both           | Daemon normalizes the webhook event, applies trigger filters, and creates a build with `repo_url` for the runner to clone |

In both cases, the runner receives the full snapshot and can apply the file-first resolution at execution time.

## Practical recommendations

- **Check in `.oore.yaml`** for reproducible builds — changes are tracked in version control
- **Use UI config** for quick experiments or when the repository doesn't have an `.oore.yaml` yet
- **Set an explicit config path** if your config file lives in a non-standard location (e.g., `ci/.oore.yaml`)
- **Review the snapshot** in the build detail view to see exactly what configuration was used for a build
