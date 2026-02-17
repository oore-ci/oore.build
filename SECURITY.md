# Security Policy

Thanks for helping keep Oore CI safe.

## Reporting a Vulnerability

Please do **not** open a public GitHub issue for security reports.

Instead, use **GitHub Security Advisories** for private disclosure:

1. Go to the repository’s “Security” tab.
2. Click “Report a vulnerability”.

If you cannot use GitHub Security Advisories, open a minimal issue that says you have
a security report and ask for a private contact channel. Do not include exploit
details, secrets, or sensitive logs in the issue.

## Scope

This policy covers:

- The self-hosted daemon (`oored`)
- The operator CLI (`oore`)
- The runner (`oore-runner`)
- The web UI (`apps/web`)

Hosted UI at `ci.oore.build` is UI-only and does not accept backend secrets.

## Supported Versions

We aim to triage security reports for the latest release on each channel:

- `stable`
- `beta`
- `alpha`

## Disclosure Process

We’ll do our best to:

- Acknowledge receipt within a few days
- Provide a timeline for a fix once impact is confirmed
- Credit reporters in release notes if desired
