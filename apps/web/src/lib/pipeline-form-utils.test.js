import { expect, test } from 'bun:test'
import { pipelineFormSchema } from '@/lib/pipeline-schema'
import { pipelineRequestFromForm } from '@/lib/pipeline-form-utils'

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
  trigger_events: ['push'],
  cancel_previous: true,
  max_concurrent: '2',
})

test('builds remote and local pipeline requests', () => {
  const remote = pipelineRequestFromForm(values, false)
  expect(remote).toMatchObject({
    name: 'Release',
    config_path: 'ci/oore.yaml',
    config_path_explicit: true,
    execution_config: {
      platforms: ['android'],
      artifact_patterns: ['build/app/outputs/flutter-apk/*.apk'],
    },
    trigger_config: { events: ['push'], branches: [] },
    concurrency: { cancel_previous: true, max_concurrent: 2 },
  })
  expect(pipelineRequestFromForm(values, true).trigger_config).toEqual({
    events: [],
    branches: [],
  })
})
