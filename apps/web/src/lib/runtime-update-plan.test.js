import { expect, test } from 'bun:test'
import {
  backendRunsRelease,
  compareReleaseVersions,
  frontendBehindBackend,
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

test('targets the backend release even when the frontend check is stale', () => {
  // The frontend check still reports the old latest release while the
  // backend already sees a newer one.
  expect(
    planCoordinatedUpdate({
      frontend: release('0.1.46', '0.1.46'),
      backend: release('0.1.46', '0.1.47'),
      frontendOffered: false,
      backendOffered: true,
      backendManaged: true,
    }),
  ).toEqual({ kind: 'ready', target: '0.1.47', backend: true, frontend: true })
})

test('brings a frontend left behind up to the backend', () => {
  // An update cut short: the backend moved to alpha.8, the frontend did not,
  // and the frontend check still names alpha.7 as the latest release.
  expect(
    planCoordinatedUpdate({
      frontend: release('0.1.50-alpha.7', '0.1.50-alpha.7'),
      backend: release('0.1.50-alpha.8', '0.1.50-alpha.8'),
      frontendOffered: false,
      backendOffered: false,
      backendManaged: true,
    }),
  ).toEqual({
    kind: 'ready',
    target: '0.1.50-alpha.8',
    backend: false,
    frontend: true,
  })
})

test('does not move a frontend past the backend', () => {
  const plan = planCoordinatedUpdate({
    frontend: release('0.1.50-alpha.8', '0.1.50-alpha.9'),
    backend: release('0.1.50-alpha.8', '0.1.50-alpha.8'),
    frontendOffered: true,
    backendOffered: false,
    backendManaged: true,
  })
  expect(plan.kind).toBe('unavailable')
  expect(plan.reason).toContain('backend has not seen it yet')
})

test('refuses when the frontend is already newer than the backend', () => {
  const plan = planCoordinatedUpdate({
    frontend: release('0.1.50-alpha.9', '0.1.50-alpha.9'),
    backend: release('0.1.50-alpha.8', '0.1.50-alpha.8'),
    frontendOffered: false,
    backendOffered: false,
    backendManaged: true,
  })
  expect(plan.kind).toBe('unavailable')
  expect(plan.reason).toContain('newer than the backend')
})

test('orders releases with numeric pre-release numbers', () => {
  expect(
    compareReleaseVersions('0.1.50-alpha.9', '0.1.50-alpha.10'),
  ).toBeLessThan(0)
  expect(
    compareReleaseVersions('v0.1.50-alpha.2', '0.1.50-beta.1'),
  ).toBeLessThan(0)
  expect(compareReleaseVersions('0.1.50-beta.3', '0.1.50')).toBeLessThan(0)
  expect(compareReleaseVersions('0.1.49', '0.1.50-alpha.1')).toBeLessThan(0)
  expect(compareReleaseVersions('v0.1.50', '0.1.50')).toBe(0)
})

test('detects a frontend behind the backend', () => {
  expect(
    frontendBehindBackend(
      { version: '0.1.50-alpha.7' },
      { version: '0.1.50-alpha.8' },
    ),
  ).toBe(true)
  expect(
    frontendBehindBackend(
      { version: '0.1.50-alpha.8' },
      { version: 'v0.1.50-alpha.8' },
    ),
  ).toBe(false)
  expect(frontendBehindBackend(undefined, { version: '0.1.50' })).toBe(false)
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
