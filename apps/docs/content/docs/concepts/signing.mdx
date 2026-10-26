---
title: 'How signing works in Oore'
status: implemented
description: 'Understand how Oore separates repository-controlled builds from runner-owned mobile signing.'
---

Oore treats repository execution and signing as different authorities.
Repository commands produce the mobile output; fixed logic owned by the Direct
macOS runner signs and verifies supported artifacts afterward.

## The signing boundary

Signing files, passwords, and API keys are encrypted by `oored` at rest. The
backend root key is a `0600` file readable only by its operating-system owner in
the daemon data root, so the database and its matching root key belong to the
same recovery set.

For an active assigned job, the trusted runner parent can retrieve signing
material with a separate job-scoped capability. It keeps that material out of
the repository checkout and scrubs managed signing values from repository
child processes. The repository does not receive signing files, passwords, or
the signing capability as build inputs.

The runner may hold signing material in its trusted parent while repository
stages run. The supported boundary is what repository child processes receive,
not a promise that the parent waits until every repository command has
finished before retrieving material.

## Android signs the completed output

An Android signing profile contains a keystore, alias, and passwords. The
repository builds an APK or App Bundle without depending on Oore-managed
signing variables. The runner then selects the configured variant, signs the
completed output in a private signer workspace, verifies it, and cleans up the
temporary signing files.

This keeps local developer signing independent. A repository can use its own
local `key.properties` path outside CI while leaving the Oore signing profile
to the runner.

## iOS adds Apple identity and device rules

iOS signing combines a certificate, provisioning profiles, Team ID, and bundle
identifier mappings. Oore supports manually uploaded assets, App Store Connect
API synchronization, and a hybrid of those paths.

The Direct runner account must have an active macOS login session. Oore creates
temporary job signing state and attempts to restore and remove it after the
job. Profiles stay in the private job workspace. This cleanup is
defense-in-depth, not hostile-code isolation for repositories already trusted
to run as that macOS account.

## What this means for you

- Do not pass Oore-managed signing credentials through repository environment
  variables or Gradle source.
- Keep an independent backup of signing keys and passwords.
- Treat the Direct runner account as sensitive and preferably dedicated and
  non-admin.
- A successful repository build can still fail later if signing or signature
  verification fails.
- An iOS install also depends on a matching, unexpired provisioning profile
  and registered-device eligibility.

## Next step

[Add Android signing](/build/sign/android) or
[configure manual iOS signing](/build/sign/ios/manual). Use the
[environment reference](/reference/config/environment) for supported
repository inputs.
