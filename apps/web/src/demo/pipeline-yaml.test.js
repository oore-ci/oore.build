import { expect, test } from 'bun:test'
import { demoExportYaml } from '@/demo/pipeline-yaml'

test('the demo export quotes YAML 1.1 keywords and writes the rollout', () => {
  const response = demoExportYaml({
    execution_config: {
      platforms: ['android'],
      commands: { pre_build: [], build: [], post_build: [] },
      env: [
        { key: 'A', value: 'yes' },
        { key: 'B', value: '~' },
        { key: 'C', value: 'plain' },
      ],
      artifact_patterns: ['build/app/outputs/flutter-apk/*.apk'],
    },
    google_play: {
      enabled: true,
      service_account_secret: 'PLAY_JSON',
      track: 'production',
      draft: false,
    },
  })
  expect(response.yaml).toContain("A: 'yes'")
  expect(response.yaml).toContain("B: '~'")
  expect(response.yaml).toContain('C: plain')
  expect(response.yaml).toContain('rollout: 1\n')
  expect(response.yaml).not.toContain('PLAY_JSON')
})
