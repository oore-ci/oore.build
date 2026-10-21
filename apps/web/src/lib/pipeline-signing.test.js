import { expect, test } from 'bun:test'
import { iosProfilesNeedResync } from '@/lib/pipeline-signing'

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
