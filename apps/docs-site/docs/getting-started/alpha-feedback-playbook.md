---
status: implemented
description: "How to file actionable alpha feedback with the minimum evidence needed to reproduce and fix issues quickly."
---

# Alpha Feedback Playbook

Use this checklist when filing public alpha feedback so issues are reproducible on first pass.

## Where to file

- Product/setup/build problems: [Alpha Test Report template](https://github.com/devaryakjha/oore.build/issues/new?template=alpha_test_report.md)
- Documentation clarity gaps: [Docs Feedback template](https://github.com/devaryakjha/oore.build/issues/new?template=docs_feedback.md)
- Security issues: private disclosure via [SECURITY.md](https://github.com/devaryakjha/oore.build/blob/master/SECURITY.md)

## Required checklist (all items)

1. Include the exact goal you were trying to complete (for example: first install, first build, webhook setup).
2. Include environment facts:
   - macOS version
   - Install channel (`stable`, `beta`, or `alpha`)
   - Setup path (`Local-only` or `Hosted UI`)
   - OIDC provider (if used)
3. Include exact reproduction steps as a numbered list.
4. Include expected result and actual result.
5. Include at least one artifact from the sections below.

Issues missing these fields may be closed as incomplete until details are provided.

## Expected artifacts

### CLI snapshots

Attach text output for:

```bash
oore version
oored version
oore status --json
oore doctor --json
```

### Build context

If the issue involves builds, include:

- Project/repo type (GitHub/GitLab, monorepo/single repo)
- Trigger path (manual/webhook/push)
- Build ID if visible
- Relevant checkout/build step logs

### Setup/auth context

If the issue involves setup/auth, include:

- The endpoint path that failed (for example `/v1/auth/local/login`)
- HTTP status code if shown
- Error `code` and `details` text if shown
- Whether access was loopback-only or remote

## Log snippet format

Use fenced blocks and keep snippets focused:

```text
Command: oore status --json
Timestamp: 2026-03-10T14:22:00Z
Output:
{ ... }
```

```text
Step: checkout
Error code: local_login_loopback_required
Details: client ip is not loopback
```

## Redaction rules

- Remove secrets: tokens, cookies, private keys, OAuth client secrets.
- Mask internal hostnames if needed.
- Keep enough context for reproduction after redaction.

## What happens after filing

1. Triage confirms reproduction scope and severity.
2. Missing artifact requests are posted once, with a concrete checklist.
3. Reproducible reports are prioritized into weekly stable release work.
