import { expect, test } from 'bun:test'
import { pipelineFormSchema } from '@/lib/pipeline-schema'
import {
  parseRunnerLabels,
  pipelineRequestFromForm,
} from '@/lib/pipeline-form-utils'

const values = pipelineFormSchema.parse({
  name: ' Release ',
  config_mode: 'explicit',
  config_path: 'ci/oore.yaml',
  platform_android: true,
  platform_ios: false,
  platform_macos: false,
  android_signing_release_enabled: false,
  android_signing_debug_enabled: false,
  ios_signing_enabled: false,
  ios_signing_mode: 'manual',
  ios_signing_export_method: 'ad_hoc',
  enable_customization: false,
  isolation: 'vm',
  runs_on: 'Xcode-16, macos, xcode-16',
  timeout_minutes: '90',
  trigger_events: ['push', 'tag_push'],
  tags: 'v*, release-*',
  paths: 'lib/\nandroid/**',
  paths_ignore: '*.md',
  schedules: [{ cron: '0 2 * * 1-5', branch: ' main ' }, { cron: '@weekly' }],
  cancel_previous: true,
  max_concurrent: '2',
  approval_required: true,
  approval_events: ['tag_push'],
})

test('builds remote and local pipeline requests', () => {
  const remote = pipelineRequestFromForm(values, {
    canChangeApproval: true,
    manualOnlyTriggers: false,
  })
  expect(remote).toMatchObject({
    name: 'Release',
    config_path: 'ci/oore.yaml',
    config_path_explicit: true,
    execution_config: {
      platforms: ['android'],
      artifact_patterns: ['build/app/outputs/flutter-apk/*.apk'],
      isolation: 'vm',
      runs_on: ['macos', 'xcode-16'],
    },
    trigger_config: {
      events: ['push', 'tag_push'],
      branches: [],
      tags: ['v*', 'release-*'],
      paths: ['lib/', 'android/**'],
      paths_ignore: ['*.md'],
      schedules: [
        { cron: '0 2 * * 1-5', branch: 'main' },
        { cron: '@weekly', branch: undefined },
      ],
    },
    concurrency: { cancel_previous: true, max_concurrent: 2 },
    timeout_minutes: 90,
    approval: { required: true, events: ['tag_push'] },
  })

  const local = pipelineRequestFromForm(values, {
    canChangeApproval: true,
    manualOnlyTriggers: true,
  })
  expect(local.trigger_config).toEqual({
    events: [],
    branches: [],
    schedules: [
      { cron: '0 2 * * 1-5', branch: 'main' },
      { cron: '@weekly', branch: undefined },
    ],
  })
})

test('tag and path filters follow the selected events', () => {
  const pushOnly = pipelineRequestFromForm(
    { ...values, trigger_events: ['push'] },
    { canChangeApproval: true, manualOnlyTriggers: false },
  )
  expect(pushOnly.trigger_config?.tags).toEqual([])
  expect(pushOnly.trigger_config?.paths).toEqual(['lib/', 'android/**'])

  const tagsOnly = pipelineRequestFromForm(
    { ...values, trigger_events: ['tag_push'] },
    { canChangeApproval: true, manualOnlyTriggers: false },
  )
  expect(tagsOnly.trigger_config?.tags).toEqual(['v*', 'release-*'])
  expect(tagsOnly.trigger_config?.paths).toEqual([])
  expect(tagsOnly.trigger_config?.paths_ignore).toEqual([])

  const anyEvent = pipelineRequestFromForm(
    { ...values, trigger_events: [] },
    { canChangeApproval: true, manualOnlyTriggers: false },
  )
  expect(anyEvent.trigger_config?.tags).toEqual(['v*', 'release-*'])
  expect(anyEvent.trigger_config?.paths).toEqual(['lib/', 'android/**'])
})

test('approval is only sent by people who may change it', () => {
  const developer = pipelineRequestFromForm(values, {
    canChangeApproval: false,
    manualOnlyTriggers: false,
  })
  expect(developer.approval).toBeUndefined()

  const off = pipelineRequestFromForm(
    { ...values, approval_required: false },
    { canChangeApproval: true, manualOnlyTriggers: false },
  )
  expect(off.approval).toEqual({ required: false, events: [] })
})

test('runner labels are lowercased, deduplicated and sorted', () => {
  expect(parseRunnerLabels(' Xcode-16, macos ,xcode-16,,')).toEqual([
    'macos',
    'xcode-16',
  ])
  expect(parseRunnerLabels('')).toEqual([])
})

test('schema rejects bad timeouts, labels and cron expressions', () => {
  const base = { ...values }
  expect(
    pipelineFormSchema.safeParse({ ...base, timeout_minutes: '0' }).success,
  ).toBe(false)
  expect(
    pipelineFormSchema.safeParse({ ...base, timeout_minutes: '1441' }).success,
  ).toBe(false)
  expect(
    pipelineFormSchema.safeParse({ ...base, runs_on: 'mac os' }).success,
  ).toBe(false)
  expect(
    pipelineFormSchema.safeParse({
      ...base,
      schedules: [{ cron: '0 2 * *' }],
    }).success,
  ).toBe(false)
  expect(
    pipelineFormSchema.safeParse({
      ...base,
      schedules: Array.from({ length: 11 }, () => ({ cron: '@daily' })),
    }).success,
  ).toBe(false)
})
