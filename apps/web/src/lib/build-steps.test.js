import assert from 'node:assert/strict'
import { test } from 'bun:test'

import { buildStepLabel, retryStepLabel } from './build-steps'

test('runner steps get readable labels', () => {
  assert.equal(buildStepLabel('checkout'), 'Check out code')
  assert.equal(buildStepLabel('vm-start'), 'Start VM')
  assert.equal(buildStepLabel('publish-play'), 'Upload to Google Play')
})

test('v1 commands are named by stage, numbered when the stage has more', () => {
  const names = ['checkout', 'pre_build-1', 'pre_build-2', 'build-1']
  assert.equal(buildStepLabel('pre_build-1', names), 'Pre-build 1')
  assert.equal(buildStepLabel('pre_build-2', names), 'Pre-build 2')
  assert.equal(buildStepLabel('build-1', names), 'Build')
  assert.equal(buildStepLabel('post_build-1'), 'Post-build')
  assert.equal(buildStepLabel('build-3'), 'Build 3')
})

test('steps the pipeline file names keep their names', () => {
  assert.equal(buildStepLabel('Run unit tests'), 'Run unit tests')
  assert.equal(buildStepLabel('build-android'), 'build-android')
})

test('retry labels are unchanged', () => {
  assert.equal(retryStepLabel('publish-play'), 'Google Play upload')
})
