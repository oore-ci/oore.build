import assert from 'node:assert/strict'
import { test } from 'bun:test'

import { expiryBand, signingExpiries } from './signing-expiry'

const DAY = 86_400

test('expiry bands at 30 and 7 days', () => {
  assert.equal(expiryBand(40 * DAY, 0), null)
  assert.equal(expiryBand(20 * DAY, 0), 'month')
  assert.equal(expiryBand(3 * DAY, 0), 'week')
  assert.equal(expiryBand(-1, 0), 'expired')
})

test('certificates and profiles that expire soon, soonest first', () => {
  const found = signingExpiries(
    [
      {
        id: 'i1',
        project_id: 'p1',
        name: 'Distribution',
        expires_at: 20 * DAY,
        profiles: [
          { bundle_id: 'com.example.app', expires_at: 5 * DAY },
          { bundle_id: 'com.example.widget', expires_at: 90 * DAY },
        ],
      },
      { id: 'i2', project_id: 'p1', name: 'Upload key' },
    ],
    0,
  )
  assert.deepEqual(
    found.map((item) => [item.what, item.band]),
    [
      ['Profile for com.example.app', 'week'],
      ['Distribution certificate', 'month'],
    ],
  )
})
