import { expect, test } from 'bun:test'
import {
  isSetupComplete,
  setupDoneCount,
  setupNext,
  setupSteps,
} from '@/lib/setup-checklist'

const fresh = {
  artifactReady: false,
  hasPipeline: false,
  hasSource: false,
  latestBuildStatus: undefined,
}

test('a new project starts at the repository step with one step done', () => {
  expect(setupDoneCount(fresh)).toBe(1)
  expect(setupNext(fresh).id).toBe('repository')
  expect(isSetupComplete(fresh)).toBe(false)
})

test('steps advance in order: repository, pipeline, build, install', () => {
  expect(setupNext({ ...fresh, hasSource: true }).id).toBe('pipeline')
  expect(setupNext({ ...fresh, hasSource: true, hasPipeline: true }).id).toBe(
    'build',
  )
  const running = {
    ...fresh,
    hasSource: true,
    hasPipeline: true,
    latestBuildStatus: 'running',
  }
  expect(setupNext(running)).toMatchObject({
    id: 'install',
    title: 'Follow your first build',
  })
  expect(setupSteps(running).map((step) => step.done)).toEqual([
    true,
    true,
    true,
    true,
    false,
  ])
})

test('a succeeded build without an app explains what to check', () => {
  const next = setupNext({
    ...fresh,
    hasSource: true,
    hasPipeline: true,
    latestBuildStatus: 'succeeded',
  })
  expect(next.id).toBe('install')
  expect(next.title).toBe('No app to install yet')
})

test('setup is complete once an installable app exists', () => {
  const done = {
    hasSource: true,
    hasPipeline: true,
    latestBuildStatus: 'succeeded',
    artifactReady: true,
  }
  expect(isSetupComplete(done)).toBe(true)
  expect(setupDoneCount(done)).toBe(5)
  expect(setupNext(done).id).toBe('done')
})
