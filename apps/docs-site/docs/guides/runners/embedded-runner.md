---
status: removed
description: 'Why embedded repository execution is unavailable in Oore CI V1.'
---

# Embedded Runner (Unavailable)

Oore V1 does not execute repository commands inside `oored`. Embedded and hybrid runner modes fail closed.

Use the supported [Direct macOS runner](/guides/runners/external-runner), which runs as a separate boot-time system LaunchDaemon under its configured non-root account and is updated with the backend toolchain.

The external process separation provides a clear operational lifecycle for builds, cancellation, updates, and Apple signing. Direct repository commands still have the permissions of the runner's macOS account; only an Owner or Admin may make the trust decision by linking a repository to a project.

There is no compatibility flag that re-enables embedded execution.
