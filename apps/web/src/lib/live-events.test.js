import assert from 'node:assert/strict'
import { test } from 'bun:test'

import {
  createTagBatcher,
  queryHasTag,
  reconnectDelay,
  tagsForLiveEvent,
} from './live-events'

test('live events map to the query tags they make stale', () => {
  assert.deepEqual(tagsForLiveEvent({ type: 'runner', runner: {} }), [
    'Runners',
  ])
  assert.deepEqual(tagsForLiveEvent({ type: 'release', release: {} }), [
    'Releases',
  ])
  assert.ok(tagsForLiveEvent({ type: 'build', build: {} }).includes('Builds'))
  assert.deepEqual(tagsForLiveEvent({ type: 'ready', ready: {} }), [])
})

test('only scoped generated queries with a matching tag are invalidated', () => {
  const tags = new Set(['Builds'])
  assert.equal(
    queryHasTag(
      { queryKey: ['i1', { _id: 'listBuilds', tags: ['Builds'] }] },
      tags,
    ),
    true,
  )
  assert.equal(
    queryHasTag(
      { queryKey: ['i1', { _id: 'listUsers', tags: ['Users'] }] },
      tags,
    ),
    false,
  )
  assert.equal(queryHasTag({ queryKey: ['setup-status'] }, tags), false)
})

test('reconnect backoff doubles and caps at 30 seconds', () => {
  assert.deepEqual(
    [1, 2, 3, 6, 20].map(reconnectDelay),
    [1_000, 2_000, 4_000, 30_000, 30_000],
  )
})

test('bursts of events flush once with every tag', async () => {
  const flushed = []
  const batcher = createTagBatcher((tags) => flushed.push([...tags].sort()), 5)
  batcher.add(['Builds'])
  batcher.add(['Runners', 'Builds'])
  await new Promise((resolve) => setTimeout(resolve, 20))
  assert.deepEqual(flushed, [['Builds', 'Runners']])
})
