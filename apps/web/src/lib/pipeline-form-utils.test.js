import { expect, test } from 'bun:test'
import { pipelineFormSchema } from '@/lib/pipeline-schema'
import {
  parseRunnerLabels,
  pipelineRepositoryFile,
  pipelineRequestFromForm,
  pipelineUpdateFromForm,
  sizeBudgetFormFields,
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
  size_budget_mode: 'warn',
  size_budget_rules: [],
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

test('the workflow is created when set and updated only when it changes', () => {
  const options = { canChangeApproval: false, manualOnlyTriggers: false }
  expect(pipelineRequestFromForm(values, options).workflow).toBeUndefined()
  const release = { ...values, workflow: ' release ' }
  expect(pipelineRequestFromForm(release, options).workflow).toBe('release')

  expect('workflow' in pipelineUpdateFromForm(values, null, options)).toBe(
    false,
  )
  expect(
    'workflow' in pipelineUpdateFromForm(release, 'release', options),
  ).toBe(false)
  expect(pipelineUpdateFromForm(release, 'adhoc', options).workflow).toBe(
    'release',
  )
  expect(pipelineUpdateFromForm(values, 'adhoc', options).workflow).toBeNull()
})

test('the repository file is the explicit path or the first auto path', () => {
  const file = (path) => ({ path, valid: true, errors: [], workflows: [] })
  const files = [file('.oore.yml'), file('.oore/release.yaml')]
  expect(
    pipelineRepositoryFile(files, {
      config_path: '.oore.yaml',
      config_path_explicit: false,
    })?.path,
  ).toBe('.oore.yml')
  expect(
    pipelineRepositoryFile(files, {
      config_path: '.oore/release.yaml',
      config_path_explicit: true,
    })?.path,
  ).toBe('.oore/release.yaml')
  expect(
    pipelineRepositoryFile(files, {
      config_path: 'ci/missing.yaml',
      config_path_explicit: true,
    }),
  ).toBeUndefined()
})

test('the size budget converts MB to bytes and back', () => {
  const budget = {
    mode: 'fail',
    rules: [
      { kind: 'aab', max_bytes: 31_457_280, max_increase_percent: 5 },
      { kind: 'ipa', max_increase_bytes: 25_000_000 },
    ],
  }
  const fields = sizeBudgetFormFields(budget)
  expect(fields.size_budget_rules[0]).toEqual({
    kind: 'aab',
    max_mb: '30',
    max_increase_mb: '',
    max_increase_percent: '5',
  })
  const request = pipelineRequestFromForm(
    { ...values, ...fields },
    { canChangeApproval: false, manualOnlyTriggers: false },
  )
  // A size set through the API keeps its exact bytes when the form saves.
  expect(request.size_budget).toEqual({
    mode: 'fail',
    rules: [
      {
        kind: 'aab',
        max_bytes: 31_457_280,
        max_increase_bytes: undefined,
        max_increase_percent: 5,
      },
      {
        kind: 'ipa',
        max_bytes: undefined,
        max_increase_bytes: 25_000_000,
        max_increase_percent: undefined,
      },
    ],
  })
  expect(sizeBudgetFormFields(undefined)).toEqual({
    size_budget_mode: 'warn',
    size_budget_rules: [],
  })
})

test('schema rejects size budget rules without a limit, bad kinds and duplicates', () => {
  const rule = { max_mb: '', max_increase_mb: '', max_increase_percent: '' }
  const result = pipelineFormSchema.safeParse({
    ...values,
    size_budget_rules: [
      { ...rule, kind: 'aab' },
      { ...rule, kind: 'AAB', max_mb: '10' },
      { ...rule, kind: 'apk', max_mb: '0', max_increase_percent: '-1' },
      { ...rule, kind: 'apk', max_increase_mb: '1' },
    ],
  })
  expect(result.success).toBe(false)
  const issues = result.error.issues.map((issue) => [
    issue.path.join('.'),
    issue.message,
  ])
  expect(issues).toEqual(
    expect.arrayContaining([
      ['size_budget_rules.0.max_mb', 'Set at least one limit'],
      [
        'size_budget_rules.1.kind',
        'Use a lowercase file extension, such as aab',
      ],
      [
        'size_budget_rules.2.max_mb',
        'Must be more than 0 and at most 1,048,576',
      ],
      ['size_budget_rules.2.max_increase_percent', 'Must be 0 to 100,000'],
      ['size_budget_rules.3.kind', 'Another rule already covers this kind'],
    ]),
  )
})

test('schema rejects sizes that round to no bytes or exceed the cap', () => {
  const rule = { kind: 'aab', max_increase_mb: '', max_increase_percent: '' }
  const parse = (max_mb) =>
    pipelineFormSchema.safeParse({
      ...values,
      size_budget_rules: [{ ...rule, max_mb }],
    }).success
  expect(parse('0.0000001')).toBe(false)
  expect(parse('2000000')).toBe(false)
  expect(parse('0.000001')).toBe(true)
  expect(parse('30')).toBe(true)
})
