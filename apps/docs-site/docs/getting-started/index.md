---
status: implemented
description: "Learn what Oore CI is and how its components work together to provide self-hosted Flutter CI."
---

# What is Oore CI?

Oore CI is a self-hosted, Flutter-first mobile CI and internal app distribution platform. It runs on your macOS hardware, builds Android, iOS, and macOS apps, and distributes them to your team — without sending source code or signing keys to a third-party service.

## How it works

Oore CI has three components:

| Component | What it does |
|---|---|
| **oored** (daemon) | Runs on your Mac. Serves the API, manages builds, stores state in SQLite. |
| **oore** (CLI) | Operator tool for setup, runner management, and diagnostics. |
| **Web UI** | React app for triggering builds, managing projects, and distributing artifacts. Hosted at `ci.oore.build` or self-hosted alongside the daemon. |

The web UI connects to your daemon over HTTPS. Your source code and signing keys stay on your hardware.

## What you can do today (V1)

- Build Flutter apps for **Android**, **iOS**, and **macOS**
- Sign Android builds with your keystore
- Sign iOS builds with certificates and provisioning profiles (manual or App Store Connect API)
- Distribute build artifacts to your team via signed download links
- Authenticate users with **any OIDC provider** (Google, Okta, Azure AD, Auth0, Keycloak)
- Control access with **role-based permissions** (owner, admin, developer, QA viewer)
- Connect **GitHub** and **GitLab** repositories with webhook-triggered builds
- Configure pipelines with a **YAML file** in your repo or through the UI

## Where to go next

Follow these pages in order to get your first instance running:

1. [**Prerequisites**](/getting-started/prerequisites) — check your system meets the requirements
2. [**Install**](/getting-started/install) — install release binaries via `curl -fsSL https://oore.build/install | bash`
3. [**Hosted UI Onboarding**](/getting-started/hosted-ui-onboarding) — connect your backend to `ci.oore.build`
4. [**Set Up Your Instance**](/getting-started/first-instance) — run the setup wizard and connect your identity provider

If you’re new, start with: [**Public Alpha (v0.1.x)**](/getting-started/public-alpha) (common blockers + fastest paths).

Before production-like usage, review: [**Known Alpha Limitations**](/getting-started/known-limitations).

When you hit friction during alpha, file reports with this checklist: [**Alpha Feedback Playbook**](/getting-started/alpha-feedback-playbook).
