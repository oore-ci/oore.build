import { expect, test } from 'bun:test'

import { storeNotesFromChangelog } from './trigger-build-store-section'

test('store notes drafted from the changelog drop authors and prefixes', () => {
  expect(
    storeNotesFromChangelog(
      [
        '- feat(login): faster sign in — Mina Patel',
        '- Fix crash on Android 15 — Sam',
        '- Keep \\*stars\\* — Lee',
        '- …and 3 more changes',
      ].join('\n'),
    ),
  ).toBe('• Faster sign in\n• Fix crash on Android 15\n• Keep *stars*')
  expect(storeNotesFromChangelog(undefined)).toBe('')
})
