import assert from 'node:assert/strict'
import { test } from 'bun:test'

import { describeRedirectPath, safeRedirectPath } from './auth-redirect'

const origin = 'https://ci.example.com'

test('sign-in returns to same-origin app pages with their query', () => {
  assert.equal(
    safeRedirectPath('/builds/b1?tab=logs#L20', origin),
    '/builds/b1?tab=logs#L20',
  )
  assert.equal(safeRedirectPath('/settings/users', origin), '/settings/users')
})

test('sign-in never redirects off the app or back into sign-in', () => {
  for (const value of [
    undefined,
    null,
    '',
    '/',
    'builds/b1',
    'https://evil.example/builds',
    '//evil.example/builds',
    '/\\evil.example',
    '/login?redirect=/builds',
    '/auth/callback',
    '/setup/owner',
  ]) {
    assert.equal(safeRedirectPath(value, origin), null, String(value))
  }
})

test('the login page names where sign-in continues to', () => {
  assert.equal(describeRedirectPath('/builds/b1?install=a'), 'this build')
  assert.equal(describeRedirectPath('/projects/p1/pipelines/x'), 'this project')
  assert.equal(describeRedirectPath('/settings/users'), 'Settings')
  assert.equal(describeRedirectPath('/elsewhere'), 'the page you opened')
})
