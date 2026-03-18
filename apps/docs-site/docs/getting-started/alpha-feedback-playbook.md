---
status: implemented
description: "How to file actionable alpha feedback with the minimum evidence needed to reproduce and fix issues quickly."
---

# Alpha Feedback Playbook

Use this checklist when filing public alpha feedback so issues are reproducible on first pass.

## 10-Minute Test Flow (Recommended)

Try these steps to exercise the core "happy path" of Oore CI.

1. **Install Oore**: Run `curl -fsSL https://oore.build/install | bash -s -- --channel alpha`.
2. **Fresh Initialize**: Run `oored run` in one terminal and `oore setup` in another.
3. **Connect a Repository**: Connect a GitHub/GitLab repo through the UI or via `oore projects create`.
4. **Trigger Build**: Start a manual build from the web dashboard.
5. **Observation**: Wait for build completion and download the generated artifact.

### Expected Output

- **Build Trigger**: A new build entry appears immediately in the UI.
- **Live Logs**: Building state starts within 30 seconds, and logs stream to the browser.
- **Artifact**: Once finished, a download button appears for the build (e.g., `build-output.zip` or `.apk`).

## Where to file

- Product/setup/build problems: [Alpha Test Report template](https://github.com/devaryakjha/oore.build/issues/new?template=alpha_test_report.md)
- Documentation clarity gaps: [Docs Feedback template](https://github.com/devaryakjha/oore.build/issues/new?template=docs_feedback.md)
- Security issues: private disclosure via [SECURITY.md](https://github.com/devaryakjha/oore.build/blob/master/SECURITY.md)

## Feedback Template (Copy/Paste)

Copy this snippet when opening an issue:

```markdown
### What were you trying to do?
<!-- e.g. First install, first build, webhook setup -->

### What happened?
<!-- e.g. Build failed at checkout step with 403 error -->

### Environment Facts
- **macOS Version**:
- **Install Channel**: alpha/beta/stable
- **Setup Path**: Local-only / Hosted UI
- **OIDC Provider**: (if used)

### Reproduction Steps
1.
2.
3.

### CLI Diagnostics
```bash
oore version
oored version
oore doctor --json
```
```

## Required checklist

1. Include the exact goal you were trying to complete.
2. Include environment facts.
3. Include exact reproduction steps as a numbered list.
4. Include expected result and actual result.
5. Include at least one diagnostic artifact (CLI snapshots, build logs).

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

## Redaction rules

- Remove secrets: tokens, cookies, private keys, OAuth client secrets.
- Mask internal hostnames if needed.
- Keep enough context for reproduction after redaction.

## What happens after filing

1. Triage confirms reproduction scope and severity.
2. Missing artifact requests are posted once, with a concrete checklist.
3. Reproducible reports are prioritized into weekly stable release work.
