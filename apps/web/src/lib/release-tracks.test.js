import assert from 'node:assert/strict'
import { test } from 'bun:test'

import {
  playVersionCodeClash,
  releasableTogether,
  releaseTracks,
  releasesByVersion,
  schedulePreview,
} from './release-tracks'

const release = (id, store, track, version, state, created, extra = {}) => ({
  id,
  store,
  track,
  version_name: version,
  state,
  created_at: created,
  allowed_actions: [],
  ...extra,
})

test('tracks show what is out and what is coming, testers first', () => {
  const tracks = releaseTracks([
    release('p1', 'google_play', 'production', '2.4.0', 'rolling', 10),
    release('p2', 'google_play', 'production', '2.5.0', 'in_review', 20),
    release('i1', 'google_play', 'internal', '2.5.0', 'complete', 15),
    release('a1', 'app_store', 'app_store', '2.5.0', 'blocked', 18),
    release('old', 'google_play', 'internal', '2.3.0', 'complete', 5),
  ])
  assert.deepEqual(
    tracks.map((track) => [
      track.key,
      track.out?.id ?? null,
      track.coming.map((item) => item.id),
    ]),
    [
      ['google_play:internal', 'i1', []],
      ['google_play:production', 'p1', ['p2']],
      ['app_store:app_store', null, ['a1']],
    ],
  )
})

test('a version groups its newest release on each track', () => {
  const groups = releasesByVersion([
    release('p', 'google_play', 'production', '2.5.0', 'approved', 20),
    release('a', 'app_store', 'app_store', '2.5.0', 'approved', 21),
    release('o', 'google_play', 'production', '2.4.0', 'complete', 10),
  ])
  assert.deepEqual(
    groups.map((group) => [group.version, group.releases.map((r) => r.id)]),
    [
      ['2.5.0', ['p', 'a']],
      ['2.4.0', ['o']],
    ],
  )
})

test('Release both needs two approved releases that can go out', () => {
  const ready = (id, store) =>
    release(id, store, 'production', '2.5.0', 'approved', 1, {
      allowed_actions: ['release_now'],
    })
  assert.equal(
    releasableTogether({
      releases: [ready('p', 'google_play'), ready('a', 'app_store')],
    }).length,
    2,
  )
  assert.equal(
    releasableTogether({ releases: [ready('p', 'google_play')] }).length,
    0,
  )
})

test('schedule preview starts after the current rollout', () => {
  assert.deepEqual(
    schedulePreview(
      {
        rollout_fraction: 0.2,
        policy: { steps: [0.05, 0.2, 0.5, 1], interval_hours: 24 },
      },
      0,
    ),
    [
      { fraction: 0.5, at: 86_400 },
      { fraction: 1, at: 172_800 },
    ],
  )
})

test('warns when the next build would not beat Play', () => {
  const play = [{ store: 'google_play', version_code: 150 }]
  assert.deepEqual(playVersionCodeClash(play, 146), { highest: 150, next: 147 })
  assert.equal(playVersionCodeClash(play, 150), null)
  assert.equal(playVersionCodeClash(play, null), null)
})
