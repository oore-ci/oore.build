# What is oore.build?

oore.build is a **self-hosted, Flutter-first mobile CI and internal app distribution platform**. It gives your team full control over build infrastructure, code signing, and artifact distribution -- all running on your own macOS hardware.

## Why oore.build?

Mobile CI is uniquely challenging. Apple's tooling requires macOS, signing is error-prone, and distributing internal builds to QA and stakeholders typically involves manual steps or expensive third-party services.

oore.build solves this by providing:

- **Flutter-first CI** -- build and test Flutter projects for Android, iOS, and macOS
- **Self-hosted** -- runs on your macOS machines, so your code and signing keys never leave your infrastructure
- **Internal distribution** -- share ad-hoc builds with your team via install/download links
- **OIDC authentication** -- integrate with your existing identity provider (Google, Okta, Azure AD, etc.)
- **Role-based access** -- owner, admin, developer, and QA viewer roles with scoped permissions

## How it works

oore.build has three main components:

| Component | Description |
|-----------|-------------|
| **`oored`** | The daemon that runs on your macOS host. Handles the API, scheduling, state, and default embedded local runner execution. |
| **`oore`** | The operator CLI for setup, administration, and runner management. |
| **Web UI** | A React frontend that connects to `oored` over HTTPS. Hosted at `ci.oore.build` or self-hosted. |

```
┌──────────────────────────────────────────────────┐
│  Web UI (ci.oore.build or self-hosted)           │
│  React 19 + TanStack Router + TanStack Query     │
└──────────────┬───────────────────────────────────┘
               │ HTTPS API
┌──────────────▼───────────────────────────────────┐
│  oored (daemon)                                   │
│  Axum HTTP server + SQLite + OIDC                 │
│  ┌──────────┐ ┌──────────┐ ┌──────────────────┐  │
│  │ Setup    │ │ Auth     │ │ Projects/Builds  │  │
│  │ Wizard   │ │ (OIDC)   │ │ + Runner APIs    │  │
│  └──────────┘ └──────────┘ └──────────────────┘  │
└──────────────────────────────────────────────────┘
               │
┌──────────────▼───────────────────────────────────┐
│  Runners (macOS build agents)                     │
│  Pull-based job scheduling over HTTPS JSON        │
└──────────────────────────────────────────────────┘
```

## Deployment model

- **Self-hosted** is the primary product. You run `oored` on your own macOS host.
- **Hosted offering** at `ci.oore.build` provides UI-only access -- your browser connects directly to your self-hosted backend. No customer code or builds run on the hosted service.

## V1 scope

The initial release focuses on:

- Flutter project builds for Android, iOS, and macOS
- Code signing and artifact publishing
- Internal distribution with install/download links
- OIDC-only authentication (no local passwords)
- Single organization per backend instance
- Process-level build isolation on macOS

::: info
oore.build is designed to be extensible. The architecture supports additional project types, backend platforms, and a native desktop client (Tauri) in future releases.
:::

## Next steps

- [Install the prerequisites](/guide/installation) to get your environment ready
- [Follow the Quick Start](/guide/quick-start) to run your first instance
- [Explore the architecture](/architecture/overview) to understand how everything fits together
