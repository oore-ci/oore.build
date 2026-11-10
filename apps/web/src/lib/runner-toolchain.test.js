import { expect, test } from 'bun:test'
import {
  checksFor,
  platformReadiness,
  runnerPlatformState,
} from '@/lib/runner-toolchain'

const git = { id: 'git', label: 'Git', platforms: [], status: 'ok' }
const sdk = {
  id: 'android_sdk',
  label: 'Android SDK',
  platforms: ['android'],
  status: 'ok',
}
const xcode = {
  id: 'xcode',
  label: 'Xcode',
  platforms: ['ios', 'macos'],
  status: 'missing',
  fix: 'Install Xcode',
}
const runner = (name, toolchain, status = 'online') => ({
  id: name,
  name,
  status,
  toolchain,
})

test('a runner is judged on the checks its platform needs', () => {
  const mac = runner('mac', [git, sdk, xcode])
  expect(checksFor(mac, 'android').map((check) => check.id)).toEqual([
    'git',
    'android_sdk',
  ])
  expect(runnerPlatformState(mac, 'android')).toBe('ready')
  expect(runnerPlatformState(mac, 'ios')).toBe('missing')
  expect(runnerPlatformState(runner('old', []), 'ios')).toBe('unknown')
})

test('any online runner that can build a platform makes it ready', () => {
  const missing = runner('a', [git, xcode])
  const ready = runner('b', [git, { ...xcode, status: 'ok' }])
  expect(platformReadiness([missing, ready], 'ios')).toMatchObject({
    state: 'ready',
    runner: { name: 'b' },
  })
  expect(platformReadiness([missing], 'ios')).toMatchObject({
    state: 'missing',
    problems: [{ id: 'xcode' }],
  })
  expect(platformReadiness([runner('c', [git], 'offline')], 'ios')).toEqual({
    state: 'offline',
  })
  expect(platformReadiness([runner('d', [])], 'ios')).toEqual({
    state: 'unknown',
  })
})
