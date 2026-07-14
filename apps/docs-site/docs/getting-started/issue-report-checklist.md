---
status: implemented
description: 'A minimal checklist to help alpha testers file reproducible reports on their first try.'
---

# Issue Report Checklist

Before filing a report on GitHub, use this checklist to ensure the team has enough information to reproduce and fix the issue quickly.

## Minimal Requirements

Every issue must include the following four sections:

1. **Reproduction Steps**: A numbered list of exactly what you clicked or typed.
2. **Environment Details**: macOS version, Oore channel (`alpha`/`beta`/`stable`), setup mode (`Local Only`/`Remote OIDC`/`Remote Trusted Proxy`), and UI path (`CLI`/`local frontend`/`hosted UI`/`split frontend`).
3. **Expected vs Actual Result**: What you thought would happen, versus what actually happened.
4. **Logs or Diagnostics**: Output from `oore doctor --json` or build logs from the UI.

## Good vs. Bad Reports

### ❌ Minimal/Bad

> "The build failed after I connected GitHub."

_Why it's bad: No logs, no steps, no environment info._

### ✅ High Quality

> "Build #145 failed at the checkout step. I am on macOS 14.4 using the `alpha` channel with a GitHub App integration."
>
> **Steps**:
>
> 1. Connect GitHub via Hosted UI.
> 2. Create a new project.
> 3. Click 'Run Build'.
>
> **Logs**:
>
> ```text
> 2026-03-10T14:22:00Z ERROR checkout: local_login_loopback_required
> ```

## Troubleshooting first

Before filing, try these high-level checks:

- Is the daemon running? (`oore status`)
- Is your browser online?
- If using a temporary tunnel, did you check the [tunnel troubleshooting example](/getting-started/public-alpha#tunnel-troubleshooting) in the Public Alpha guide?

## Where to file

- **Bug Reports**: [Open a new issue](https://github.com/devaryakjha/oore.build/issues/new) using the **Alpha Test Report** template.
- **Security Findings**: Follow [SECURITY.md](https://github.com/devaryakjha/oore.build/blob/master/SECURITY.md) for private disclosure.
