import { expect, test } from 'bun:test'
import { pipelineFormSchema } from '@/lib/pipeline-schema'
import {
  downloadFileName,
  pipelinesReadingPath,
  repositoryFilePath,
  repositoryFileView,
  yamlExportRequest,
} from '@/lib/pipeline-yaml-export'

const base = {
  name: 'Release',
  config_mode: 'auto',
  platform_android: true,
  platform_ios: true,
  platform_macos: false,
  android_signing_release_enabled: false,
  android_signing_debug_enabled: false,
  ios_signing_enabled: false,
  ios_signing_mode: 'manual',
  ios_signing_export_method: 'ad_hoc',
  enable_customization: true,
  pre_build_commands: 'dart run build_runner build',
  env_vars: 'TOKEN=${{ secrets.TOKEN }}',
  isolation: 'host',
  timeout_minutes: '60',
  trigger_events: ['push'],
  branches: 'main',
  schedules: [{ cron: '@daily' }],
  cancel_previous: false,
  approval_required: false,
  approval_events: [],
}
const parsed = pipelineFormSchema.parse(base)
// Mid-edit, before the form validates the field.
const values = { ...parsed, build_number_offset: '12x' }

const play = {
  enabled: true,
  service_account_secret: 'PLAY_JSON',
  track: 'beta',
  draft: false,
}
const appStore = {
  enabled: false,
  destination: 'testflight',
  testflight_groups: ['QA'],
  release_notes: [],
}

test('sends the form, its triggers and only the stores that are on', () => {
  const request = yamlExportRequest(values, {
    manualOnlyTriggers: false,
    publish: { googlePlay: play, appStore },
  })
  expect(request.execution_config.platforms).toEqual(['android', 'ios'])
  expect(request.execution_config.commands.pre_build).toEqual([
    'dart run build_runner build',
  ])
  // A half-typed offset is read as none, as the plan panel does.
  expect(request.execution_config.build_number_offset).toBe(0)
  // Secret references go as written; the daemon never expands them.
  expect(request.execution_config.env).toEqual([
    { key: 'TOKEN', value: '${{ secrets.TOKEN }}' },
  ])
  expect(request.trigger_config).toMatchObject({
    events: ['push'],
    branches: ['main'],
  })
  expect(request.google_play).toBe(play)
  expect(request.app_store).toBeUndefined()
})

test('a manual-only repository exports no triggers', () => {
  const request = yamlExportRequest(values, {
    manualOnlyTriggers: true,
    publish: undefined,
  })
  expect(request.trigger_config).toBeUndefined()
  expect(request.google_play).toBeUndefined()
})

test('names the file where the runner looks for it', () => {
  expect(repositoryFilePath(values)).toBe('.oore.yaml')
  const explicit = pipelineFormSchema.parse({
    ...base,
    config_mode: 'explicit',
    config_path: ' ci/pipelines/release.yaml ',
  })
  expect(repositoryFilePath(explicit)).toBe('ci/pipelines/release.yaml')
  expect(downloadFileName('ci/pipelines/release.yaml')).toBe('release.yaml')
  expect(downloadFileName('.oore.yaml')).toBe('.oore.yaml')
})

test('warns about the other pipelines that read the same file', () => {
  const others = [
    { name: 'Nightly', config_path: '.oore.yaml', config_path_explicit: false },
    { name: 'iOS', config_path: ' ci/ios.yaml ', config_path_explicit: true },
    { name: 'Docs', config_path: 'ci/docs.yaml', config_path_explicit: true },
  ]
  expect(pipelinesReadingPath('.oore.yaml', others)).toEqual(['Nightly'])
  expect(pipelinesReadingPath('ci/ios.yaml', others)).toEqual(['iOS'])
  expect(pipelinesReadingPath('ci/other.yaml', others)).toEqual([])
})

test('the pane shows errors, never a stale or missing file', () => {
  const ready = {
    valid: true,
    errors: [],
    yaml: 'version: 2\n',
    warnings: ['w'],
    notes: [],
  }
  expect(repositoryFileView({ isError: true, result: ready })).toEqual({
    kind: 'error',
  })
  expect(repositoryFileView({ isError: false, result: undefined })).toEqual({
    kind: 'loading',
  })
  expect(
    repositoryFileView({
      isError: false,
      result: { valid: false, errors: ['bad'], warnings: [], notes: [] },
    }),
  ).toEqual({ kind: 'invalid', errors: ['bad'] })
  expect(
    repositoryFileView({
      isError: false,
      result: { valid: false, errors: [], warnings: [], notes: [] },
    }).kind,
  ).toBe('invalid')
  expect(repositoryFileView({ isError: false, result: ready })).toEqual({
    kind: 'ready',
    yaml: 'version: 2\n',
    warnings: ['w'],
  })
})
