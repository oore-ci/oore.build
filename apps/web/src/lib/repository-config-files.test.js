import { expect, test } from 'bun:test'
import {
  addableConfigFiles,
  describeConfigFile,
  pipelineHealthBanner,
  pipelineProblemBanner,
  unusedConfigFiles,
} from '@/lib/repository-config-files'

const file = (path, overrides = {}) => ({
  path,
  valid: true,
  errors: [],
  workflows: [],
  suggested_name: path,
  pipelines: [],
  seen_at: 0,
  ...overrides,
})

test('only files no pipeline uses are offered, and only valid ones can be added', () => {
  const response = {
    project_id: 'p1',
    truncated: false,
    files: [
      file('.oore.yaml', { pipelines: [{ id: 'pl1', name: 'Default' }] }),
      file('.oore.release.yaml'),
      file('.oore/broken.yaml', { valid: false, errors: ['nope'] }),
    ],
  }
  expect(unusedConfigFiles(response).map((f) => f.path)).toEqual([
    '.oore.release.yaml',
    '.oore/broken.yaml',
  ])
  expect(addableConfigFiles(response).map((f) => f.path)).toEqual([
    '.oore.release.yaml',
  ])
  expect(unusedConfigFiles(undefined)).toEqual([])
})

test('a config file row says what it builds or why it cannot', () => {
  expect(
    describeConfigFile(file('a', { workflows: ['adhoc', 'release'] })),
  ).toBe('Workflows: adhoc, release')
  expect(describeConfigFile(file('a'))).toBe('Builds as one workflow')
  expect(
    describeConfigFile(file('a', { valid: false, errors: ['bad indent'] })),
  ).toBe('bad indent')
})

test('the health banner treats a missing file as information', () => {
  const health = (state) => ({
    state,
    message: 'why',
    branch: 'main',
    commit_sha: 'abc',
    checked_at: 0,
  })
  expect(pipelineHealthBanner(health('ok'))).toBeNull()
  expect(pipelineHealthBanner(undefined)).toBeNull()
  expect(pipelineHealthBanner(health('file_missing')).variant).toBe('default')
  expect(pipelineHealthBanner(health('file_invalid')).variant).toBe(
    'destructive',
  )
  expect(pipelineHealthBanner(health('workflow_missing')).description).toBe(
    'why',
  )
})

test('a missing file fails a pipeline that names a default workflow', () => {
  const health = {
    state: 'file_missing',
    message: 'ci/gone.yaml is not on main, so builds use the settings saved in Oore',
    branch: 'main',
    commit_sha: 'abc',
    checked_at: 0,
  }
  const banner = pipelineHealthBanner(health, {
    config_path: 'ci/gone.yaml',
    workflow: 'release',
  })
  expect(banner.variant).toBe('destructive')
  expect(banner.title).toBe('No workflow file on main')
  expect(banner.description).toContain('ci/gone.yaml')
  expect(
    pipelineHealthBanner(health, { config_path: 'ci/gone.yaml', workflow: '' })
      .variant,
  ).toBe('default')
})

test('a live read of the repository gives the same banner shape', () => {
  expect(
    pipelineProblemBanner(
      { kind: 'invalid-file', path: '.oore.yaml', errors: ['a', 'b'] },
      'main',
    ),
  ).toEqual({
    title: 'Config file is invalid',
    description: '.oore.yaml on main has errors, so builds from it fail: a; b',
    variant: 'destructive',
  })
  expect(
    pipelineProblemBanner(
      { kind: 'unknown-workflow', path: '.oore.yaml', workflow: 'beta' },
      'main',
    ).title,
  ).toBe('Workflow is missing')
})
