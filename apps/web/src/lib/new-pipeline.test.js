import assert from 'node:assert/strict'
import { test } from 'bun:test'

import { defaultValues, fileValues, planLine } from './new-pipeline'

const file = {
  path: '.oore.release.yaml',
  valid: true,
  errors: [],
  workflows: ['beta', 'production'],
  execution: {
    platforms: ['android', 'ios'],
    flutter_version: '3.24.3',
    commands: { pre_build: ['flutter pub get'], build: [], post_build: [] },
    isolation: 'vm',
    platform_build_args: { android: [], ios: [], macos: [] },
    platform_commands: {
      android: 'flutter build appbundle --release',
      ios: 'flutter build ipa --release',
    },
    env_keys: [],
    artifact_patterns: ['build/ios/ipa/*.ipa'],
  },
}

test('a file names the pipeline, keeps its path and picks a workflow', () => {
  const values = fileValues(file, undefined)
  assert.equal(values.name, 'Release')
  assert.equal(values.config_mode, 'explicit')
  assert.equal(values.config_path, '.oore.release.yaml')
  assert.equal(values.workflow, 'beta')
  assert.equal(fileValues(file, 'Store').name, 'Store')
  assert.equal(
    fileValues({ ...file, workflows: ['only'] }, undefined).workflow,
    '',
  )
})

test('the plan line counts steps the way the planner lists them', () => {
  // pub get added by the planner, the file's pub get, two platform builds.
  assert.equal(
    planLine(fileValues(file, undefined)),
    'Builds Android and iOS, 4 commands',
  )
  assert.equal(planLine(defaultValues(undefined)), 'Builds Android, 2 commands')
})

test('defaults build a debug APK, or the platforms an app without Android has', () => {
  assert.equal(defaultValues(['android', 'ios']).name, 'Debug APK')
  const ios = defaultValues(['ios'])
  assert.equal(ios.name, 'iOS build')
  assert.equal(ios.platform_android, false)
  assert.equal(ios.platform_ios, true)
  assert.equal(defaultValues(['ios', 'macos']).name, 'App build')
})
