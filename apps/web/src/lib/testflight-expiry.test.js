import { expect, test } from 'bun:test'

import { testflightExpiries } from './testflight-expiry'

const DAY = 86_400
const release = (id, track, createdDaysAgo, extra = {}) => ({
  id,
  project_id: 'p1',
  store: 'app_store',
  track,
  state: 'complete',
  created_at: 100 * DAY - createdDaysAgo * DAY,
  ...extra,
})

test('the newest TestFlight build per group warns before 90 days', () => {
  const found = testflightExpiries(
    [
      release('old', 'Beta', 88),
      release('new', 'Beta', 20),
      release('qa', 'QA', 84),
      release('store', 'app_store', 89),
      release('gone', 'Ops', 89),
      release('long-gone', 'Old', 120),
    ],
    100 * DAY,
  )
  expect(found.map((item) => [item.release.id, item.band])).toEqual([
    ['gone', 'days'],
    ['qa', 'fortnight'],
  ])
})
