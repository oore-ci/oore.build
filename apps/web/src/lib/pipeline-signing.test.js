import { expect, test } from 'bun:test'
import {
  buildIosSigningPayload,
  iosProfilesNeedResync,
} from '@/lib/pipeline-signing'

const stored = {
  pipeline_id: 'pipe-1',
  enabled: true,
  mode: 'api',
  export_method: 'ad_hoc',
  team_id: 'TEAM1234',
  bundle_ids: ['com.example.app'],
  has_p12: false,
  has_p12_password: false,
  has_api_key: true,
  api_key_id: 'KEY123',
  api_issuer_id: 'issuer',
  provisioning_profiles: [],
}

const payload = {
  enabled: true,
  mode: 'api',
  export_method: 'app_store',
  team_id: 'TEAM1234',
  bundle_ids: ['com.example.app'],
}

test('a distribution change in API or hybrid mode needs a profile sync', () => {
  expect(iosProfilesNeedResync(payload, stored)).toBe(true)
  expect(iosProfilesNeedResync({ ...payload, mode: 'hybrid' }, stored)).toBe(
    true,
  )
})

test('no sync without a distribution change, API credentials or a stored state', () => {
  expect(
    iosProfilesNeedResync({ ...payload, export_method: 'ad_hoc' }, stored),
  ).toBe(false)
  expect(
    iosProfilesNeedResync({ ...payload, export_method: undefined }, stored),
  ).toBe(false)
  expect(iosProfilesNeedResync({ ...payload, mode: 'manual' }, stored)).toBe(
    false,
  )
  expect(iosProfilesNeedResync({ ...payload, enabled: false }, stored)).toBe(
    false,
  )
  expect(iosProfilesNeedResync(payload, undefined)).toBe(false)
})

const formValues = {
  platform_ios: true,
  ios_signing_enabled: true,
  ios_signing_mode: 'api',
  ios_signing_export_method: 'app_store',
  ios_signing_team_id: 'TEAM1234',
  ios_signing_bundle_ids: 'com.example.app',
  ios_signing_p12_password: '',
  ios_signing_api_key_id: '',
  ios_signing_api_issuer_id: '',
  ios_signing_api_key_source: 'shared-1',
}
const noFiles = { p12File: null, apiKeyFile: null, profileFiles: {} }

test('a shared App Store Connect key needs no key of the pipeline', async () => {
  const { errors, payload } = await buildIosSigningPayload(formValues, noFiles)
  expect(errors).toEqual([])
  expect(payload?.shared_api_key_id).toBe('shared-1')
  expect(payload?.api_credentials).toBeUndefined()
})

test('switching back to the pipeline key clears the shared one', async () => {
  const previous = {
    ...stored,
    export_method: 'app_store',
    shared_api_key: { id: 'shared-1' },
  }
  const { errors, payload } = await buildIosSigningPayload(
    { ...formValues, ios_signing_api_key_source: 'own' },
    noFiles,
    previous,
  )
  expect(errors).toEqual([])
  expect(payload?.shared_api_key_id).toBe('')
})

test('manual mode keeps the stored key choice', async () => {
  const { payload } = await buildIosSigningPayload(
    {
      ...formValues,
      ios_signing_mode: 'manual',
      ios_signing_enabled: false,
    },
    noFiles,
    stored,
  )
  expect(payload?.shared_api_key_id).toBeUndefined()
})
