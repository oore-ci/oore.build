import { expect, test } from 'bun:test'
import {
  mapWithLimit,
  parseEmailList,
  projectRoleFor,
} from '@/lib/project-access'

test('parseEmailList reads lines, commas, semicolons and mail-client names', () => {
  expect(
    parseEmailList(
      'Priya <Priya@Acme.dev>, rahul@acme.dev\nsam@example.com; rahul@acme.dev\n\n  not-an-email  ',
    ),
  ).toEqual(['priya@acme.dev', 'rahul@acme.dev', 'sam@example.com'])
  expect(parseEmailList('Morgan Lee')).toEqual([])
})

test('projectRoleFor keeps QA viewers read-only', () => {
  expect(projectRoleFor('qa_viewer', 'maintainer')).toBe('viewer')
  expect(projectRoleFor('developer', 'maintainer')).toBe('maintainer')
})

test('mapWithLimit keeps order, caps concurrency and reports failures', async () => {
  let inFlight = 0
  let peak = 0
  const results = await mapWithLimit([1, 2, 3, 4, 5, 6], 2, async (n) => {
    inFlight++
    peak = Math.max(peak, inFlight)
    await new Promise((resolve) => setTimeout(resolve, 5))
    inFlight--
    if (n === 4) throw new Error('four')
    return n * 10
  })
  expect(peak).toBe(2)
  expect(
    results.map((r) => (r.status === 'fulfilled' ? r.value : r.reason.message)),
  ).toEqual([10, 20, 30, 'four', 50, 60])
})
