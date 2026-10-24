import { expect, test } from 'bun:test'
import { pipelineFormSchema } from '@/lib/pipeline-schema'
import {
  executionConfigFromForm,
  triggerConfigFromForm,
} from '@/lib/pipeline-form-utils'
import { yamlExportRequest } from '@/lib/pipeline-yaml-export'
import {
  changedFields,
  importOutcome,
  yamlImportRequest,
} from '@/lib/pipeline-yaml-import'

const values = pipelineFormSchema.parse({
  name: 'Release',
  config_mode: 'auto',
  platform_android: true,
  platform_ios: false,
  platform_macos: false,
  android_signing_release_enabled: false,
  android_signing_debug_enabled: false,
  ios_signing_enabled: false,
  ios_signing_mode: 'manual',
  ios_signing_export_method: 'ad_hoc',
  enable_customization: false,
  artifact_patterns: 'build/app/outputs/flutter-apk/*.apk',
  isolation: 'host',
  timeout_minutes: '60',
  trigger_events: [],
  branches: '',
  schedules: [{ cron: '@daily', branch: '' }],
  cancel_previous: false,
  approval_required: false,
  approval_events: [],
  size_budget_mode: 'warn',
  size_budget_rules: [],
})

/** What the daemon returns for a file the form can hold. */
const imported = {
  platforms: ['android', 'ios'],
  flutter_version: '3.24.5',
  commands: {
    pre_build: ['dart run build_runner build', 'flutter analyze'],
    build: [],
    post_build: ['flutter test'],
  },
  platform_build_args: { android: ['--split-per-abi'], ios: [], macos: [] },
  platform_commands: { ios: 'flutter build ipa --release' },
  env: [{ key: 'SENTRY_DSN', value: '${{ secrets.SENTRY_DSN }}' }],
  artifact_patterns: [
    'build/app/outputs/flutter-apk/*.apk',
    'build/ios/ipa/*.ipa',
  ],
  build_number_offset: 7,
  runs_on: ['xcode-16'],
  isolation: 'vm',
}
const response = {
  valid: true,
  representable: true,
  errors: [],
  unsupported: [],
  notes: ['Step names are not kept'],
  execution_config: imported,
}

function outcome(result, current = values, manualOnlyTriggers = false) {
  return importOutcome({
    isError: false,
    result,
    current,
    manualOnlyTriggers,
  })
}

test('the import request carries the text and the settings the export used', () => {
  const exportRequest = yamlExportRequest(values, {
    manualOnlyTriggers: false,
    publish: undefined,
  })
  const request = yamlImportRequest('version: 2\n', exportRequest)
  expect(request.yaml).toBe('version: 2\n')
  expect(request.execution_config).toEqual(exportRequest.execution_config)
  expect(request.trigger_config).toEqual(exportRequest.trigger_config)
})

test('a file the form can hold becomes form fields with no loss', () => {
  const result = outcome(response)
  expect(result.kind).toBe('ready')
  expect(result.notes).toEqual(['Step names are not kept'])
  const next = { ...values, ...result.fields }
  // Commands only reach the settings with customization on.
  expect(next.enable_customization).toBe(true)
  // The form, read the way it saves, is what the daemon returned.
  expect(executionConfigFromForm(next)).toEqual({
    ...imported,
    platform_build_args: { android: ['--split-per-abi'], ios: [], macos: [] },
    platform_commands: {
      android: undefined,
      ios: 'flutter build ipa --release',
      macos: undefined,
    },
  })
  // Read again, the form already matches: nothing to set.
  expect(outcome(response, next)).toEqual({
    kind: 'ready',
    fields: {},
    notes: ['Step names are not kept'],
  })
})

test('only fields that change are set', () => {
  const fields = changedFields(
    { flutter_version: '', runs_on: 'arm64', trigger_events: [] },
    values,
  )
  expect(fields).toEqual({ runs_on: 'arm64' })
})

test('trigger settings from `on` reach the trigger fields', () => {
  const trigger_config = {
    events: ['push', 'tag_push'],
    branches: ['main', 'release/*'],
    tags: ['v*'],
    paths: ['lib/**'],
    paths_ignore: [],
    schedules: [{ cron: '@daily' }],
  }
  const result = outcome({ ...response, trigger_config })
  expect(result.kind).toBe('ready')
  const next = { ...values, ...result.fields }
  expect(triggerConfigFromForm(next, false)).toEqual({
    ...trigger_config,
    schedules: [{ cron: '@daily', branch: undefined }],
  })
})

test('a file the form cannot hold leaves the form alone and says why', () => {
  const result = outcome({
    ...response,
    representable: false,
    execution_config: undefined,
    unsupported: ['workflows (release, adhoc): …'],
  })
  expect(result).toEqual({
    kind: 'unsupported',
    unsupported: ['workflows (release, adhoc): …'],
    notes: ['Step names are not kept'],
  })
  // A manual-only repository has no trigger fields for `on`.
  const manual = outcome(
    { ...response, trigger_config: { events: ['push'], branches: [] } },
    values,
    true,
  )
  expect(manual.kind).toBe('unsupported')
  expect(manual.unsupported[0]).toContain('only builds manually')
})

test('invalid files, failures and pending checks never touch the form', () => {
  expect(
    outcome({
      valid: false,
      representable: false,
      errors: ['YAML parse error: …'],
      unsupported: [],
      notes: [],
    }),
  ).toEqual({ kind: 'invalid', errors: ['YAML parse error: …'] })
  expect(outcome(undefined)).toEqual({ kind: 'checking' })
  expect(
    importOutcome({
      isError: true,
      result: response,
      current: values,
      manualOnlyTriggers: false,
    }),
  ).toEqual({ kind: 'error' })
})
