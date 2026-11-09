import assert from 'node:assert/strict'
import { test } from 'bun:test'

import {
  buildIdentityParts,
  buildProgress,
  buildVersionLabel,
  remainingLabel,
  samePushLeaders,
} from './build-identity'

test('version reads like the stores show it', () => {
  assert.equal(
    buildVersionLabel({ version_name: '2.5.1', version_code: 146 }),
    '2.5.1 (146)',
  )
  assert.equal(buildVersionLabel({ version_name: '2.5.1' }), '2.5.1')
  assert.equal(buildVersionLabel({ version_code: 146 }), null)
})

test('identity leaves out what the runner has not reported', () => {
  assert.deepEqual(
    buildIdentityParts({
      version_name: '2.5.1',
      version_code: 146,
      commit_title: 'Fix saved addresses',
      commit_author: 'Mina Patel',
    }),
    ['2.5.1 (146)', 'Fix saved addresses', 'Mina Patel'],
  )
  assert.deepEqual(buildIdentityParts({}), [])
})

const run = (id, seconds, steps) => ({
  id,
  status: 'succeeded',
  started_at: 1000,
  finished_at: 1000 + seconds,
  step_results: steps.map((name) => ({ name })),
})

test('progress uses the median run and the latest step list', () => {
  const history = [
    run('a', 600, ['checkout', 'build', 'publish']),
    run('b', 400, ['checkout', 'build']),
    run('c', 500, ['checkout', 'build']),
  ]
  assert.deepEqual(
    buildProgress({
      history,
      buildId: 'x',
      currentStep: 'build',
      elapsedSeconds: 260,
    }),
    { step: 2, total: 3, typicalSeconds: 500, remainingSeconds: 240 },
  )
  assert.equal(
    buildProgress({
      history: history.slice(0, 1),
      buildId: 'x',
      currentStep: null,
      elapsedSeconds: 0,
    }),
    null,
  )
})

test('remaining time reads plainly', () => {
  assert.equal(remainingLabel(240), 'about 4 min left')
  assert.equal(remainingLabel(30), 'under a minute left')
  assert.equal(remainingLabel(-5), 'taking longer than usual')
})

test('one push that fanned out groups under its first build', () => {
  const push = (id, pipeline, created, workflow) => ({
    id,
    project_id: 'p',
    pipeline_id: pipeline,
    workflow,
    trigger_type: 'webhook',
    commit_sha: 'abc',
    branch: 'main',
    created_at: created,
  })
  const leaders = samePushLeaders([
    push('b2', 'android', 105),
    push('b1', 'ios', 100),
    push('b3', 'ios', 900),
  ])
  assert.equal(leaders.get('b2')?.id, 'b1')
  assert.equal(leaders.has('b1'), false)
  assert.equal(leaders.has('b3'), false)
})
