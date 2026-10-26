---
title: "Oore's security model"
status: implemented
description: 'Understand how access mode, identity, authorization, repository trust, and scoped secrets protect an Oore instance.'
---

Oore uses several security boundaries that solve different problems. Network
access establishes where a request may come from, authentication identifies
the person or runner, authorization limits actions, and repository linking
decides which code may execute.

## Access mode defines the first boundary

Local Only accepts passwordless local login over effective loopback. It is the
default path for one Mac and does not expose an ordinary non-loopback sign-in.

External Access requires a protected non-loopback path and one configured
authentication mode:

- External Access (OIDC) lets Oore perform standards-based provider sign-in
  with PKCE and issue the Oore session.
- External Access (Trusted Proxy) accepts identity only when the immediate
  peer, configured identity header, and shared proof all pass.

A ready External Access instance does not gain a normal passwordless bypass
because a request reaches loopback. Host-authorized `oore recovery` is a
separate, single-use recovery path.

## The selected backend owns identity and authorization

The installed, self-hosted, and hosted browser clients all defer to the
selected backend. The hosted client is static UI only; it does not provide a
hosted Oore backend or server-side credential store.

`oored` enforces instance and project permissions for every protected action.
Instance roles and project roles are separate. Hiding a resource with a
privacy-preserving not-found response and rejecting an insufficient project
role are backend decisions, not UI-only guards.

## Repository trust is explicit

An instance owner or admin trusts a repository when creating a project or
changing its linked source. A Direct macOS runner then executes that
repository's checkout, dependencies, and commands with the runner account's
host permissions.

Oore does not add a second repository approval list and does not claim
same-account containment for hostile code. Verified source events from
external forks or ambiguous repository identities are ignored instead of run
automatically.

## Secrets use narrower capabilities

Stored provider, OIDC, and signing secrets are encrypted with the backend's
file-backed root key. The key file uses `0600` permissions and must remain
paired with the state database for recovery.

Runners and users receive narrower capabilities for jobs, signing, uploads,
downloads, or install sessions only after the backend authorizes the action.
Repository child processes do not receive managed signing files, passwords, or
capability tokens. Short-lived capabilities reduce exposure, but they remain
secrets until they expire.

## What this means for you

- Protect non-loopback traffic with TLS and the chosen External Access
  boundary.
- Keep the backend database and matching root key together for recovery.
- Give instance and project access separately and use the least privilege that
  fits the job.
- Run the Direct macOS runner under a dedicated non-admin account where
  practical.
- Treat browser sessions and generated artifact links as credentials.
- Use disposable macOS virtual machines, outside the V1 runner contract, when
  untrusted contributions require strong isolation.

## Next step

Choose an access model under [Team and access](/team/access), then review the
exact [role and permission reference](/reference/roles).
