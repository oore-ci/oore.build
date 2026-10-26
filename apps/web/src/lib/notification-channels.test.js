import { expect, test } from 'bun:test'
import {
  formatSlackMentions,
  parseSlackMentions,
} from '@/lib/notification-channels'

test('parseSlackMentions reads one mapping per line', () => {
  expect(
    parseSlackMentions(' Dev@Example.com U0123ABCD\n\noctocat\tW0OCTO\n'),
  ).toEqual({
    ok: true,
    mentions: [
      { identity: 'dev@example.com', member_id: 'U0123ABCD' },
      { identity: 'octocat', member_id: 'W0OCTO' },
    ],
  })
  expect(parseSlackMentions('')).toEqual({ ok: true, mentions: [] })
})

test('parseSlackMentions names the line that is wrong', () => {
  expect(parseSlackMentions('dev@example.com')).toEqual({
    ok: false,
    error: 'Line 1: use "email-or-git-username U0123ABCD"',
  })
  expect(parseSlackMentions('a U0A\ndev@example.com @dev')).toEqual({
    ok: false,
    error: 'Line 2: @dev is not a Slack member ID (for example U0123ABCD)',
  })
  expect(parseSlackMentions('a U0A\nA U0B').ok).toBe(false)
})

test('parseSlackMentions keeps = and , inside an email', () => {
  expect(parseSlackMentions('a=b,c@example.com U0ABC')).toEqual({
    ok: true,
    mentions: [{ identity: 'a=b,c@example.com', member_id: 'U0ABC' }],
  })
  expect(parseSlackMentions('octocat=U0OCTO').ok).toBe(false)
})

test('parseSlackMentions enforces the 500 entry cap', () => {
  const lines = (count) =>
    Array.from({ length: count }, (_, i) => `user${i} U0${i}A`).join('\n')
  expect(parseSlackMentions(lines(500)).ok).toBe(true)
  expect(parseSlackMentions(lines(501))).toEqual({
    ok: false,
    error: 'List at most 500 people (501 listed)',
  })
})

test('formatSlackMentions round-trips saved mappings', () => {
  const mentions = [
    { identity: 'octocat', member_id: 'U0OCTO' },
    { identity: 'a=b,c@example.com', member_id: 'U0ABC' },
  ]
  expect(formatSlackMentions(mentions)).toBe(
    'octocat U0OCTO\na=b,c@example.com U0ABC',
  )
  expect(parseSlackMentions(formatSlackMentions(mentions))).toEqual({
    ok: true,
    mentions,
  })
  expect(formatSlackMentions(undefined)).toBe('')
})
