# iOS Signing (Ad Hoc)

oore.build supports pipeline-scoped iOS ad hoc signing with secure secret handling and runner-side ephemeral keychain setup.

## Modes

1. `manual`
- Upload `.p12` certificate + password.
- Upload `.mobileprovision` profile for each bundle ID.

2. `api`
- Provide App Store Connect API key metadata (`key_id`, `issuer_id`) and `.p8` private key.
- Use sync action to discover devices, create/regenerate profiles, and materialize signing assets.

3. `hybrid`
- Keep manual certificate material.
- Use API for device registration and profile synchronization.

## Manual Mode Walkthrough

1. Open pipeline create/edit form.
2. Enable **iOS Signing**.
3. Select `manual` mode.
4. Enter Team ID and bundle IDs (main + extension IDs if any).
5. Upload `.p12` and enter password.
6. Upload one `.mobileprovision` per bundle ID.
7. Save pipeline.

## API Mode Walkthrough

1. Enable **iOS Signing**.
2. Select `api` mode.
3. Enter Team ID and bundle IDs.
4. Enter API key ID + issuer ID.
5. Upload App Store Connect `.p8` private key.
6. Save pipeline.
7. Run **Sync Profiles** from pipeline edit page to generate/update signing assets.

## Device Registration Flow

1. In pipeline edit page, open **Registered iOS Devices**.
2. Enter device name and UDID.
3. Click **Register Device**.
4. For API/hybrid pipelines, profile sync is triggered automatically when possible.
5. Use **Sync Profiles** to force regeneration and verify warnings/status.

## Runner Behavior

When a build executes Flutter iOS commands and iOS signing is configured:

- Runner fetches `/v1/runners/{runner_id}/jobs/{job_id}/ios-signing`.
- Runner creates temporary keychain and imports `.p12`.
- Runner installs provisioning profiles for build scope.
- Runner generates `ExportOptions.plist` and enforces signed IPA export path.
- Runner cleans up temporary keychain/profile files on completion/failure/cancel.

## Security Notes

- iOS signing secrets are encrypted at rest in backend storage.
- Public pipeline signing APIs return only non-secret metadata.
- Runner bundle retrieval is restricted to assigned runner/job ownership.
- Sensitive env/log output is masked.

## Troubleshooting

- `incomplete_signing_profile` on build fetch:
  - run iOS signing sync
  - verify required mode assets are present
- `apple_api_error` during register/sync:
  - verify API key scope/role and Team access
  - verify bundle IDs exist in Apple account
- IPA unsigned/missing:
  - ensure pipeline includes Flutter iOS build command
  - avoid conflicting custom command overrides that bypass signed export flow
