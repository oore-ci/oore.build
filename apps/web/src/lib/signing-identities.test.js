import { expect, test } from 'bun:test'
import {
  FOLLOW_FILE,
  SIGNING_IDENTITY_NAME,
  expiryState,
  signingIdentityExpiry,
  signingIdentitySummary,
  signingIdentityUsage,
  workflowSettingsFormValues,
  workflowSettingsRequest,
} from '@/lib/signing-identities'
import { signingPlatforms, workflowRows } from '@/lib/pipeline-workflow-rows'

const ios = {
  id: 'sid-1',
  project_id: 'p',
  name: 'coin-app-store',
  kind: 'ios_distribution',
  team_id: 'A1B2C3D4E5',
  export_method: 'app_store',
  expires_at: 2_000,
  profiles: [
    { bundle_id: 'com.acme.coin', expires_at: 1_500 },
    { bundle_id: 'com.acme.coin.widgets' },
  ],
  used_by: [
    { pipeline_id: 'pl', pipeline_name: 'Release', workflow: 'release' },
  ],
  created_at: 0,
  updated_at: 0,
}

test('identity names follow workflow names', () => {
  expect(SIGNING_IDENTITY_NAME.test('coin-app_store2')).toBe(true)
  expect(SIGNING_IDENTITY_NAME.test('-coin')).toBe(false)
  expect(SIGNING_IDENTITY_NAME.test('coin store')).toBe(false)
  expect(SIGNING_IDENTITY_NAME.test('a'.repeat(65))).toBe(false)
})

test('summaries describe the material without secrets', () => {
  expect(signingIdentitySummary(ios)).toBe(
    'App Store · Team A1B2C3D4E5 · 2 profiles',
  )
  expect(
    signingIdentitySummary({
      ...ios,
      kind: 'android_keystore',
      filename: 'upload.jks',
      key_alias: 'upload',
    }),
  ).toBe('upload.jks · alias upload')
  expect(signingIdentityUsage(ios)).toBe('Picked by Release (release)')
  expect(signingIdentityUsage({ ...ios, used_by: [] })).toBe(
    'Not picked in any workflow settings',
  )
})

test('the soonest certificate or profile expiry decides the warning', () => {
  expect(signingIdentityExpiry(ios)).toBe(1_500)
  expect(expiryState(1_500, 2_000)).toBe('expired')
  expect(expiryState(1_500, 1_500 - 86_400)).toBe('soon')
  expect(expiryState(1_500 + 40 * 86_400, 1_500)).toBe('ok')
})

test('workflow settings round-trip and only store what differs from the file', () => {
  const values = workflowSettingsFormValues({
    workflow: 'release',
    ios_identity_id: 'sid-1',
    publish_enabled: false,
    approval_required: true,
  })
  expect(values).toEqual({
    android_identity_id: FOLLOW_FILE,
    ios_identity_id: 'sid-1',
    publish_enabled: false,
    approval: 'required',
  })
  expect(workflowSettingsRequest(values)).toEqual({
    ios_identity_id: 'sid-1',
    publish_enabled: false,
    approval_required: true,
  })
  // Everything left to the file clears the row.
  expect(workflowSettingsRequest(workflowSettingsFormValues())).toEqual({})
  expect(
    workflowSettingsRequest({
      ...workflowSettingsFormValues(),
      approval: 'not_required',
    }),
  ).toEqual({ approval_required: false })
})

test('rows keep settings for workflows the file dropped', () => {
  const listing = {
    pipeline_id: 'pl',
    state: 'valid',
    default_workflow: 'release',
    workflows: [
      {
        name: 'release',
        platforms: ['ios'],
        triggers: [],
        uses_pipeline_triggers: true,
        publishes: [],
      },
    ],
  }
  const rows = workflowRows(listing, [
    { workflow: 'release', publish_enabled: false },
    { workflow: 'old', ios_identity_id: 'sid-1' },
  ])
  expect(rows.map((row) => [row.name, row.isDefault, !!row.summary])).toEqual([
    ['release', true, true],
    ['old', false, false],
  ])
  expect(signingPlatforms(rows[0])).toEqual(['ios'])
  expect(signingPlatforms(rows[1])).toEqual(['android', 'ios'])
})
