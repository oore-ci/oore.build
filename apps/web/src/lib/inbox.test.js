import assert from 'node:assert/strict'
import { test } from 'bun:test'

import { itemsToAnnounce, unreadCount, wantsInboxItem } from './inbox'

const preferences = {
  approvals: true,
  browser: true,
  followed_releases: false,
  my_builds: 'failed',
}

test('unread counts only items after the last look', () => {
  assert.equal(unreadCount([{ at: 10 }, { at: 20 }, { at: 30 }], 20), 1)
})

test('each kind follows its own choice', () => {
  assert.equal(wantsInboxItem(preferences, { kind: 'approval' }), true)
  assert.equal(wantsInboxItem(preferences, { kind: 'release' }), false)
  assert.equal(
    wantsInboxItem(
      { ...preferences, my_builds: 'off' },
      { kind: 'failed_build' },
    ),
    false,
  )
})

test('browser notifications skip the first load and items already shown', () => {
  const items = [
    { id: 'a', kind: 'approval' },
    { id: 'b', kind: 'failed_build' },
    { id: 'c', kind: 'release' },
  ]
  assert.deepEqual(itemsToAnnounce(items, null, preferences), [])
  assert.deepEqual(
    itemsToAnnounce(items, new Set(['a']), preferences).map((item) => item.id),
    ['b'],
  )
  assert.deepEqual(
    itemsToAnnounce(items, new Set(), { ...preferences, browser: false }),
    [],
  )
})
