import { expect, test } from 'bun:test'
import {
  backendRunsRelease,
  planCoordinatedUpdate,
  releaseIsOffered,
} from '@/lib/runtime-update-plan'

const release = (version, latest) => ({
  version,
  latest_version: latest,
  update_available: version !== latest,
  managed_service: true,
})

test('a release check is not offered once health reports the new version', () => {
  const stale = release('0.1.46', '0.1.47')
  expect(releaseIsOffered(stale, { version: '0.1.46' })).toBe(true)
  expect(releaseIsOffered(stale, { version: 'v0.1.47' })).toBe(false)
})

test('a pre-release backend matches only its own release', () => {
  const ready = { version: '0.1.47-alpha.2', package_version: '0.1.47' }
  expect(backendRunsRelease(ready, '0.1.47-alpha.2')).toBe(true)
  expect(backendRunsRelease(ready, '0.1.47-alpha.3')).toBe(false)
  expect(
    backendRunsRelease(
      { version: '0.1.47', package_version: '0.1.46' },
      '0.1.47',
    ),
  ).toBe(false)
})

test('updates both components to one matching release', () => {
  expect(
    planCoordinatedUpdate({
      frontend: release('0.1.46', '0.1.47'),
      backend: release('0.1.46', '0.1.47'),
      frontendOffered: true,
      backendOffered: true,
      backendManaged: true,
    }),
  ).toEqual({ kind: 'ready', target: '0.1.47', backend: true, frontend: true })
})

test('updates only the component behind the other', () => {
  expect(
    planCoordinatedUpdate({
      frontend: release('0.1.47', '0.1.47'),
      backend: release('0.1.46', '0.1.47'),
      frontendOffered: false,
      backendOffered: true,
      backendManaged: true,
    }),
  ).toEqual({ kind: 'ready', target: '0.1.47', backend: true, frontend: false })
})

test('refuses a pair that would end on different versions', () => {
  const plan = planCoordinatedUpdate({
    frontend: release('0.1.46', '0.1.46'),
    backend: release('0.1.46', '0.1.47'),
    frontendOffered: false,
    backendOffered: true,
    backendManaged: true,
  })
  expect(plan.kind).toBe('unavailable')
})

test('falls back to per-component updates for split deployments', () => {
  const plan = planCoordinatedUpdate({
    frontend: undefined,
    backend: release('0.1.46', '0.1.47'),
    frontendOffered: false,
    backendOffered: true,
    backendManaged: true,
  })
  expect(plan.kind).toBe('unavailable')
})
