---
status: implemented
description: 'Learn what Oore CI is and how its components work together to provide self-hosted Flutter CI.'
---

# What is Oore CI?

Oore CI is a self-hosted, Flutter-first mobile CI and internal app distribution platform. It runs on your macOS hardware, builds Android, iOS, and macOS apps, and distributes them to your team — without sending source code or signing keys to a third-party service.

## How it works

Oore CI has three components:

| Component          | What it does                                                                                                                                                                                       |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **oored** (daemon) | Runs on your Mac. Serves the API, manages builds, stores state in SQLite.                                                                                                                          |
| **oore** (CLI)     | Operator tool for setup, runner management, and diagnostics.                                                                                                                                       |
| **Web UI**         | React client for setup, triggering builds, managing projects, and distributing artifacts. Hosted at `ci.oore.build` or self-hosted; the backend still owns setup, auth, data, and build execution. |

The web UI connects to your daemon over HTTPS. Your source code and signing keys stay on your hardware.

## What you can do today (V1)

- Build Flutter apps for **Android**, **iOS**, and **macOS**
- Sign Android builds with your keystore
- Sign iOS builds with certificates and provisioning profiles (manual or App Store Connect API)
- Distribute build artifacts to your team via signed download links
- Authenticate users with **Local Only**, **Remote OIDC**, or **Remote Trusted Proxy** modes
- Control access with **role-based permissions** (owner, admin, developer, QA viewer)
- Connect **GitHub** and **GitLab** repositories with webhook-triggered builds
- Configure pipelines with a **YAML file** in your repo or through the UI

## Where to go next

Follow these pages in order to get your first instance running:

1. [**Prerequisites**](/getting-started/prerequisites) — check your system meets the requirements
2. [**Install**](/getting-started/install) — install release binaries via `curl -fsSL https://oore.build/install | bash`
3. [**Set Up Your Instance**](/getting-started/first-instance) — choose Local Only, Remote OIDC, or Remote Trusted Proxy
4. [**Hosted UI Onboarding**](/getting-started/hosted-ui-onboarding) — connect an HTTPS-reachable backend to `ci.oore.build`

If you are testing a prerelease, review the [release channels](/operations/release-channels).

Before production-like usage, review the [known limitations](/operations/known-limitations).

When you hit friction, use the [issue report checklist](/operations/report-an-issue).
