---
layout: home
status: implemented
description: "Documentation for Oore CI, a self-hosted Flutter-first mobile CI and internal app distribution platform."
hero:
  name: Oore CI
  text: Flutter-first mobile CI
  tagline: Self-hosted build system with internal app distribution. OIDC or trusted-proxy auth for remote access, macOS-native, no vendor lock-in.
  actions:
    - theme: brand
      text: Get Started
      link: /getting-started/
    - theme: alt
      text: View on GitHub
      link: https://github.com/devaryakjha/oore.build

features:
  - title: Self-Hosted CI
    details: Run your own build infrastructure on macOS. Full control over signing keys, build artifacts, and distribution — nothing leaves your network unless you choose.
  - title: Flutter-First
    details: Purpose-built for Flutter. Android, iOS, and macOS builds with automatic Flutter version management via FVM and file-first pipeline configuration.
  - title: Remote Auth
    details: OIDC or trusted-proxy auth for non-loopback access, plus loopback-only local login for local-first onboarding. Use your IdP directly or put Oore behind Warpgate — no local passwords, ever.
---

## Who is this for?

<div class="vp-doc">

| I want to... | Start here |
|---|---|
| **Set up Oore CI for the first time** | [Getting Started](/getting-started/) |
| **Configure my OIDC provider** | [OIDC Guides](/guides/oidc/) |
| **Split backend and web UI across hosts** | [Split Backend and Frontend](/operations/split-roles) |
| **Deploy on a Mac Studio behind NetBird + Warpgate** | [Mac Studio + NetBird + Warpgate](/operations/mac-studio-netbird-warpgate) |
| **Look up an API endpoint or CLI flag** | [Reference](/reference/api/) |

</div>
