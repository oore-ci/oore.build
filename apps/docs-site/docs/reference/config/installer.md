---
status: implemented
description: 'Installer roles, automation controls, version selection, and environment variables.'
---

# Installer reference

The default installer creates a complete local instance. The controls below are for pinned, automated, backend-only, or frontend-only installations.

## Advanced installer

```bash
curl -fsSL https://oore.build/install | bash -s -- --advanced
```

| Role       | Host support   | Installs                                                      |
| ---------- | -------------- | ------------------------------------------------------------- |
| `auto`     | macOS or Linux | Prompts on macOS; selects `frontend` on Linux                 |
| `all`      | macOS          | Daemon, CLI, runner binary, frontend launcher, and web assets |
| `backend`  | macOS          | Daemon and CLI, including runner commands                     |
| `frontend` | macOS or Linux | Frontend launcher and web assets                              |

`full` remains a compatibility alias for `all`. New automation should use the role names above. For complete split-host examples, see [Split backend and frontend roles](/operations/split-roles).

## Pin a version

```bash
curl -fsSL https://oore.build/install | OORE_VERSION=v0.2.0 bash
```

`OORE_VERSION` overrides channel selection.

## Run without prompts

```bash
curl -fsSL https://oore.build/install | \
  OORE_NONINTERACTIVE=1 \
  OORE_START_DAEMON=true \
  bash
```

When `OORE_NONINTERACTIVE=1`, explicitly provide every value required by the selected role. Daemon startup is skipped unless `OORE_START_DAEMON` is set.

## Environment variables

| Variable                                             | Default                           | Purpose                                                                 |
| ---------------------------------------------------- | --------------------------------- | ----------------------------------------------------------------------- |
| `OORE_VERSION`                                       | `latest`                          | Release selector: `latest` or a tag such as `v0.2.0`                    |
| `OORE_CHANNEL`                                       | `stable`                          | Channel used when `OORE_VERSION=latest`: `stable`, `beta`, or `alpha`   |
| `OORE_INSTALL_MODE`                                  | `auto`                            | Advanced role: `auto`, `all`, `backend`, or `frontend`                  |
| `OORE_INSTALL_ROOT`                                  | `~/.oore`                         | Installation directory                                                  |
| `OORE_GITHUB_REPO`                                   | `oore-ci/oore.build`              | Repository used to download release assets                              |
| `OORE_RELEASE_BASE_URL`                              | GitHub Releases                   | Origin containing versioned release assets                              |
| `OORE_RELEASE_INDEX_BASE_URL`                        | `https://releases.oore.build`     | Release discovery origin                                                |
| `OORE_RELEASE_MANIFEST_URL`                          | Channel manifest                  | Exact latest-channel manifest override                                  |
| `OORE_NONINTERACTIVE`                                | `0`                               | Disable prompts when set to `1`                                         |
| `OORE_OPEN_BROWSER`                                  | Local interactive installs        | Control whether the local web root opens after installation             |
| `OORE_DAEMON_LISTEN`                                 | Derived from `OORE_DAEMON_URL`    | Daemon listen address for `all` and `backend` roles                     |
| `OORE_START_DAEMON`                                  | unset                             | Start the daemon during a non-interactive install                       |
| `OORE_INSTALL_DAEMON_SERVICE`                        | unset                             | Install and start the managed daemon service                            |
| `OORE_PUBLIC_URL`                                    | unset                             | Browser-visible HTTPS URL passed to the daemon service                  |
| `OORE_CORS_ORIGINS`                                  | `OORE_PUBLIC_URL` when set        | Comma-separated browser origins                                         |
| `OORE_DAEMON_URL`                                    | `http://127.0.0.1:8787`           | Daemon URL used by setup helpers                                        |
| `OORE_WEB_BACKEND_URL`                               | `OORE_DAEMON_URL`                 | Backend URL proxied by `oore-web`                                       |
| `OORE_WEB_BACKEND_TRANSPORT_PROTECTED`               | `false`                           | Assert that an encrypted transport protects a remote HTTP backend hop   |
| `OORE_WEB_BROWSER_TRANSPORT_PROTECTED`               | `false`                           | Assert that encrypted ingress protects a non-loopback HTTP web listener |
| `OORE_FRONTEND_PAIRING_CODE`                         | unset                             | Single-use code from `oore frontend invite`                             |
| `OORE_SETUP_OWNER_EMAIL`                             | unset                             | Initial owner email for Trusted Proxy setup                             |
| `OORE_SETUP_PROXY_PRESET`                            | `generic`                         | Trusted Proxy preset: `generic`, `warpgate`, or `custom`                |
| `OORE_SETUP_USER_EMAIL_HEADER`                       | unset                             | Identity header for the `custom` proxy preset                           |
| `OORE_TRUSTED_PROXY_SHARED_SECRET`                   | unset                             | Backend Trusted Proxy proof supplied during installation                |
| `OORE_TRUSTED_PROXY_SHARED_SECRET_FILE`              | Managed secret path               | File containing the backend Trusted Proxy proof                         |
| `OORE_TRUSTED_PROXY_CIDRS`                           | unset                             | Trusted proxy or frontend peer CIDRs                                    |
| `OORE_WEB_TRUSTED_PROXY_USER_EMAIL_HEADER`           | Preset-derived                    | Identity header `oore-web` may forward after proof validation           |
| `OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET`      | unset                             | Auth proxy to `oore-web` proof                                          |
| `OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE` | Managed secret path               | File containing the upstream proof                                      |
| `OORE_WEB_UPSTREAM_TRUSTED_PROXY_SECRET_HEADER`      | `x-oore-web-trusted-proxy-secret` | Header carrying the upstream proof                                      |
| `OORE_LOCAL_WEB_MODE`                                | unset                             | Local frontend behavior: `off`, `run`, or `login`                       |
| `OORE_LOCAL_WEB_LISTEN`                              | `127.0.0.1:4173`                  | `oore-web` listen address                                               |
| `OORE_ENABLE_LINGER`                                 | unset                             | Enable systemd lingering for a Linux frontend service                   |

The two Trusted Proxy proof values must be different. Prefer `OORE_FRONTEND_PAIRING_CODE` over copying proof files manually.
