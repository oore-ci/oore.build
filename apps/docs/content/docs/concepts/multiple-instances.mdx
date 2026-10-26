---
title: 'How multiple instances stay separate'
status: implemented
description: 'Understand how one browser keeps backend connections, sessions, and cached data scoped by instance.'
---

One Oore browser client can connect to several backends, but switching the
active instance changes the API authority. It does not merge the backends or
their data.

## The browser keeps a namespaced connection

The browser persists the instance registry and ordinary authenticated session
under an instance namespace in local storage. A setup session is temporary and
uses session storage instead.

Each registered backend keeps its own:

- origin and display label
- authenticated user and session expiry
- cached project, build, runner, and settings data

Switching the active instance changes which namespace and backend the client
uses. A session for one backend is never sent to another.

## Adding a connection does not validate the backend

Adding an instance stores the connection immediately. It does not run a
readiness check first, so an incorrect or unreachable origin can still appear
in the switcher and fail when the browser tries to use it.

The current UI cannot edit a saved backend origin or remove a saved instance.
Add a corrected connection as a new instance and sign in to that backend
independently.

## The hosted client follows the same model

`ci.oore.build` is an optional static browser client for HTTPS-reachable
backends. The browser talks directly to each selected backend and stores the
namespaced registry and authenticated sessions locally. The hosted service
does not hold a server-side instance directory, shared session, or copy of
backend data.

Every backend remains responsible for its own users, projects, runners,
artifacts, access mode, and availability.

## What this means for you

- Sign in independently to each backend.
- Check the active instance before changing a project or setting.
- Expect a newly added connection to fail later if its origin is wrong or
  unreachable.
- Verify the backend origin before saving it because the current UI cannot
  edit or remove the entry.
- Clearing browser local storage removes the saved registry and ordinary
  sessions from that browser.
- Switching instances does not move or copy builds, runners, or artifacts.

## Next step

[Add another instance](/operate/instances/add), then
[switch between instances](/operate/instances/switch). Access within each
backend follows the exact [role reference](/reference/roles).
