import { describe, expect, it } from 'vitest'

import type { PipelineFormValues } from '@/lib/pipeline-schema'
import {
  defaultArtifactPatterns,
  executionConfigFromForm,
  hasSigningFileChanges,
  previewPlatformCommands,
} from '@/lib/pipeline-form-utils'

describe('hasSigningFileChanges', () => {
  it('detects signing files even when ordinary form values are unchanged', () => {
    const file = new File(['secret'], 'signing.p12')

    expect(hasSigningFileChanges([null, null], {})).toBe(false)
    expect(hasSigningFileChanges([file, null], {})).toBe(true)
    expect(hasSigningFileChanges([null], { 'com.example.app': file })).toBe(
      true,
    )
  })
})

const defaults: PipelineFormValues = {
  name: 'Debug APK',
  config_mode: 'auto',
  config_path: '.oore.yaml',
  platform_android: true,
  platform_ios: false,
  platform_macos: false,
  android_signing_release_enabled: false,
  android_signing_release_store_password: '',
  android_signing_release_key_alias: '',
  android_signing_release_key_password: '',
  android_signing_debug_enabled: false,
  android_signing_debug_store_password: '',
  android_signing_debug_key_alias: '',
  android_signing_debug_key_password: '',
  ios_signing_enabled: false,
  ios_signing_mode: 'manual',
  ios_signing_team_id: '',
  ios_signing_bundle_ids: '',
  ios_signing_p12_password: '',
  ios_signing_api_key_id: '',
  ios_signing_api_issuer_id: '',
  flutter_version: '',
  enable_customization: true,
  pre_build_commands: '',
  build_commands: '',
  post_build_commands: '',
  android_build_args: '',
  ios_build_args: '',
  macos_build_args: '',
  android_command_override: 'flutter build apk --debug',
  ios_command_override: '',
  macos_command_override: '',
  env_vars: '',
  artifact_patterns: '',
  branches: '',
  max_concurrent: undefined,
}

describe('pipeline form defaults', () => {
  it('uses Flutter output paths for default artifacts', () => {
    expect(defaultArtifactPatterns(['android', 'ios', 'macos'])).toEqual([
      'build/app/outputs/flutter-apk/*.apk',
      'build/ios/ipa/*.ipa',
      'build/macos/Build/Products/Release/*.app',
    ])
  })

  it('keeps the Quick Debug APK command in the pipeline payload preview', () => {
    expect(previewPlatformCommands(defaults)).toEqual([
      'flutter build apk --debug',
    ])
  })

  it('keeps independent args, env, and artifacts when custom commands are off', () => {
    const config = executionConfigFromForm({
      ...defaults,
      enable_customization: false,
      build_commands: 'echo ignored',
      android_build_args: '--target=lib/main_adhoc.dart',
      env_vars: 'APP_FLAVOR=adhoc',
      artifact_patterns: 'build/app/outputs/bundle/release/*.aab',
    })

    expect(config.commands.build).toEqual([])
    expect(config.platform_build_args?.android).toEqual([
      '--target=lib/main_adhoc.dart',
    ])
    expect(config.env).toEqual([{ key: 'APP_FLAVOR', value: 'adhoc' }])
    expect(config.artifact_patterns).toEqual([
      'build/app/outputs/bundle/release/*.aab',
    ])
  })
})
