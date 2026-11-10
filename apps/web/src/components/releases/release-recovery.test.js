import assert from 'node:assert/strict'
import { test } from 'bun:test'

import { monthGrid } from './release-calendar'
import { canShipHotfix, previousRelease } from './release-recovery'
import { lastGoodBuild } from '../build-details/compare-last-good'

function release(id, overrides) {
  return {
    id,
    store: 'google_play',
    track: 'production',
    state: 'complete',
    build_id: `b-${id}`,
    created_at: 100,
    ...overrides,
  }
}

test('monthGrid starts on Monday and fills whole weeks', () => {
  // November 2026 starts on a Sunday.
  const cells = monthGrid(2026, 10)
  assert.equal(cells.length % 7, 0)
  assert.equal(
    cells.slice(0, 6).every((cell) => cell === null),
    true,
  )
  assert.equal(cells[6].getDate(), 1)
  assert.equal(cells.filter(Boolean).length, 30)
})

test('previousRelease is the last earlier version on the same track', () => {
  const halted = release('now', { state: 'halted', created_at: 300 })
  const others = [
    release('old', { created_at: 100 }),
    release('prev', { created_at: 200 }),
    release('ios', { store: 'app_store', track: 'app_store', created_at: 250 }),
    release('failed', { state: 'failed', created_at: 280 }),
  ]
  assert.equal(previousRelease(halted, others)?.id, 'prev')
  assert.equal(previousRelease(release('first'), []), null)
})

test('a hotfix follows a release that is out', () => {
  assert.equal(canShipHotfix(release('a', { state: 'rolling' })), true)
  assert.equal(canShipHotfix(release('b', { state: 'in_review' })), false)
  assert.equal(canShipHotfix(release('c', { build_id: null })), false)
})

test('lastGoodBuild prefers the same branch', () => {
  const failed = { id: 'f', branch: 'main', created_at: 500 }
  const builds = [
    { id: 'later', status: 'succeeded', branch: 'main', created_at: 600 },
    { id: 'other', status: 'succeeded', branch: 'dev', created_at: 450 },
    { id: 'main', status: 'succeeded', branch: 'main', created_at: 400 },
  ]
  assert.equal(lastGoodBuild(failed, builds)?.id, 'main')
  assert.equal(lastGoodBuild({ ...failed, branch: 'x' }, builds)?.id, 'other')
  assert.equal(lastGoodBuild(failed, []), null)
})
