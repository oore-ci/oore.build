---
title: 'How Oore is structured'
status: implemented
description: 'Understand how Oore separates browser and operator surfaces, backend control, and build execution.'
---

Oore has one backend authority and a separate build executor. They can run on
the same Mac, which is the default, without becoming the same process or trust
boundary.

## The control and execution model

| Part                | Responsibility                                                                                             |
| ------------------- | ---------------------------------------------------------------------------------------------------------- |
| Browser client      | Presents setup, projects, builds, team access, and settings for the selected backend.                      |
| `oore`              | Handles operator work such as setup, diagnostics, recovery, and runner administration.                     |
| `oored`             | Owns authentication, authorization, projects, pipelines, build state, settings, and storage authorization. |
| Direct macOS runner | Checks out trusted repositories, runs their commands, signs supported outputs, and uploads artifacts.      |

`oored` is the control plane. Its state belongs to one backend instance and is
stored in that instance's SQLite database. The Direct macOS runner is the
execution plane. It claims eligible work from `oored` and reports results back.

The supported product does not execute repository commands inside `oored`.
Embedded and hybrid execution are unavailable.

## The default still uses separate responsibilities

The default complete install places `oored`, `oore`, the local browser client,
and a managed Direct macOS runner on one customer-owned Mac. Local artifact
storage also lives with that backend unless the operator configures S3 or R2.

Keeping these responsibilities separate matters even on one host:

- the backend decides whether a request or runner action is authorized
- the runner executes repository code with its macOS account's permissions
- the browser displays the selected backend's state
- artifact bytes use the configured backend-local or object-storage path

A Direct macOS runner may instead run on another Mac. The backend remains the
authority, while repository execution moves to that runner account.

## Browser clients do not become backends

The installed client, a self-hosted static client, and `ci.oore.build` use the
same backend contracts. The hosted client serves static UI assets only. It does
not run `oored`, proxy every customer's API, execute builds, or provide
server-side storage for Oore credentials and artifacts.

The browser talks to the selected backend directly or through a configured
product frontend proxy. Authentication, authorization, build state, and
storage remain owned by that backend.

## What this means for you

- Moving the browser client does not move the backend or its data.
- Moving a runner does not change which backend authorizes the job.
- Adding object storage moves artifact bytes, not project or authentication
  state.
- One browser can select several backends, but those instances do not share
  users, projects, runners, or artifacts.
- Repository code is trusted to run under the Direct runner account; process
  separation is not hostile-code isolation.

## Next step

Use [Install Oore](/start/install) for the default one-Mac path. For exact HTTP
operations exposed by the backend, use the [API reference](/reference/api).
