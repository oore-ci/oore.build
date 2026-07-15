---
status: implemented
description: 'Current public-alpha limitations and constraints for Oore CI v0.1.x.'
---

# Known Alpha Limitations (v0.1.x)

This page lists intentional constraints and known gaps for the current public alpha.

## Product maturity

- Oore CI is in **public alpha**. APIs, config formats, and CLI flags may change across `v0.1.x`.
- The `stable` channel means default install/update channel, not 1.0 production maturity.

## Platform scope

- The V1 backend runtime target is **macOS only**.
- Linux/Windows backend support is not part of the current alpha scope.

## Access model constraints

- `https://ci.oore.build` is **UI-only**. It does not host your daemon, source code, or signing keys.
- Hosted UI requires an **HTTPS-reachable backend URL**.
- A page served from `https://ci.oore.build` cannot connect directly to `http://127.0.0.1:*`.

## Authentication constraints

- For non-loopback/remote access, configure remote authentication before team usage.
- Loopback-only local login is available for local-first onboarding in Local Only mode.
- Oore CI does not support local password-based auth.

## Operational expectations

- No formal uptime/SLA guarantees are provided in alpha.
- You should expect rough edges and occasional setup/documentation gaps while feedback is incorporated.

## What we need feedback on

- Onboarding clarity and first-run friction
- Build/signing reliability on real projects
- Runner diagnostics and operational visibility
- Missing workflows that block day-to-day usage

File actionable feedback in GitHub Issues:

- <https://github.com/oore-ci/oore.build/issues>
- [Alpha Feedback Playbook](/getting-started/alpha-feedback-playbook)
