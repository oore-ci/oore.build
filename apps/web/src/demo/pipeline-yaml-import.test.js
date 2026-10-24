import { expect, test } from 'bun:test'
import { demoPipelines } from '@/demo/data/pipelines'
import { demoExportYaml } from '@/demo/pipeline-yaml'
import { demoImportYaml, parseDemoYaml } from '@/demo/pipeline-yaml-import'

const androidRelease = demoPipelines.find((p) => p.name === 'Android Release')
const request = {
  execution_config: {
    ...androidRelease.execution_config,
    env: [{ key: 'SENTRY_DSN', value: '${{ secrets.SENTRY_DSN }}' }],
  },
  trigger_config: {
    events: ['push'],
    branches: ['main', 'release/*'],
    schedules: [{ cron: '@daily' }],
  },
  google_play: {
    enabled: true,
    service_account_secret: 'PLAY_SERVICE_ACCOUNT',
    track: 'production',
    rollout_percent: 5,
    draft: false,
  },
}
const yaml = demoExportYaml(request).yaml

test('the demo pipeline reads back as its own settings', () => {
  const response = demoImportYaml({ yaml, ...request })
  expect(response.unsupported).toEqual([])
  expect(response.notes).toEqual([])
  expect(response.representable).toBe(true)
  expect(response.execution_config).toMatchObject({
    platforms: ['android'],
    flutter_version: '3.24.3',
    commands: request.execution_config.commands,
    platform_build_args: { android: ['--split-per-abi'] },
    env: [{ key: 'SENTRY_DSN', value: '${{ secrets.SENTRY_DSN }}' }],
    artifact_patterns: ['build/app/outputs/flutter-apk/*.apk'],
    runs_on: ['macos', 'release'],
  })
  // The Play bundle Oore adds is not an override.
  expect(response.execution_config.platform_commands.android).toBeUndefined()
  // Settings that already say the same `on` stay as they are.
  expect(response.trigger_config).toBe(request.trigger_config)
  expect(response.google_play).toEqual({
    track: 'production',
    rollout_percent: 5,
    draft: false,
  })
  // Exported again, the file is the same.
  expect(
    demoExportYaml({
      ...request,
      execution_config: response.execution_config,
      trigger_config: response.trigger_config,
    }).yaml,
  ).toBe(yaml)
})

test('edits in the demo file reach the settings', () => {
  const edited = yaml
    .replace(
      '  - build\n',
      '  - name: Lint\n    run: dart format --set-exit-if-changed .\n  - build\n',
    )
    .replace('      - main\n', '      - main\n      - develop\n')
  const response = demoImportYaml({ yaml: edited, ...request })
  expect(response.representable).toBe(true)
  expect(response.execution_config.commands.pre_build).toContain(
    'dart format --set-exit-if-changed .',
  )
  expect(response.trigger_config.branches).toEqual([
    'main',
    'develop',
    'release/*',
  ])
  expect(response.trigger_config.schedules).toEqual([{ cron: '@daily' }])
})

test('the demo lists what the form cannot hold and rejects broken files', () => {
  const workflows = demoImportYaml({
    yaml: `${yaml}workflows:\n  nightly: {}\n`,
    ...request,
  })
  expect(workflows.representable).toBe(false)
  expect(workflows.execution_config).toBeUndefined()
  expect(workflows.unsupported[0]).toContain('workflows (nightly)')

  const rollout = demoImportYaml({
    yaml: yaml.replace('rollout: 0.05', 'rollout: 0.5'),
    ...request,
  })
  expect(rollout.representable).toBe(false)
  expect(rollout.unsupported[0]).toContain('production track at 5%')

  const broken = demoImportYaml({ yaml: `${yaml}deploy: now\n`, ...request })
  expect(broken.valid).toBe(false)
  expect(broken.errors[0]).toContain('unknown field `deploy`')
})

test('the demo reads literal blocks and anchors as the daemon does', () => {
  const script = demoImportYaml({
    yaml: yaml.replace(
      '  - build\n',
      '  - name: Script\n    run: |\n      echo one\n      echo two\n  - build\n',
    ),
    ...request,
  })
  expect(script.unsupported[0]).toContain("'Script' runs several lines")
  const parsed = parseDemoYaml("a: &k\n  - 'it''s'\nb: *k\n")
  expect(parsed.map.b).toBe(parsed.map.a)
  expect(parsed.map.a.items[0].text).toBe("it's")
})
