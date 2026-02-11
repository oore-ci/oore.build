import { PIPELINE_IDS, PROJECT_IDS, ago } from '../seed'
import type { Pipeline } from '@/lib/types'

export const demoPipelines: Array<Pipeline> = [
  {
    id: PIPELINE_IDS.shopAndroid,
    project_id: PROJECT_IDS.flutterShop,
    name: 'Android Release',
    config_path: '.oore/android-release.yaml',
    config_path_explicit: true,
    execution_config: {
      platforms: ['android'],
      flutter_version: '3.24.3',
      commands: {
        pre_build: ['flutter pub get', 'flutter analyze --no-fatal-infos'],
        build: ['flutter build apk --release'],
        post_build: ['flutter test --coverage'],
      },
      platform_build_args: { android: ['--split-per-abi'], ios: [], macos: [] },
      artifact_patterns: ['build/app/outputs/flutter-apk/*.apk'],
    },
    trigger_config: {
      events: ['push', 'pull_request'],
      branches: ['main', 'release/*'],
    },
    concurrency: { cancel_previous: true, max_concurrent: 2 },
    enabled: true,
    created_at: ago(86400 * 55),
    updated_at: ago(86400 * 3),
  },
  {
    id: PIPELINE_IDS.shopIos,
    project_id: PROJECT_IDS.flutterShop,
    name: 'iOS Release',
    config_path: '.oore/ios-release.yaml',
    config_path_explicit: true,
    execution_config: {
      platforms: ['ios'],
      flutter_version: '3.24.3',
      commands: {
        pre_build: ['flutter pub get'],
        build: [
          'flutter build ipa --release --export-options-plist=ios/ExportOptions.plist',
        ],
        post_build: [],
      },
      artifact_patterns: ['build/ios/ipa/*.ipa'],
    },
    trigger_config: {
      events: ['push'],
      branches: ['main'],
    },
    concurrency: { cancel_previous: false, max_concurrent: 1 },
    enabled: true,
    created_at: ago(86400 * 50),
    updated_at: ago(86400 * 5),
  },
  {
    id: PIPELINE_IDS.adminAndroid,
    project_id: PROJECT_IDS.internalAdmin,
    name: 'Debug Build',
    config_path: '.oore/debug.yaml',
    config_path_explicit: false,
    execution_config: {
      platforms: ['android'],
      flutter_version: '3.24.3',
      commands: {
        pre_build: ['flutter pub get'],
        build: ['flutter build apk --debug'],
        post_build: [],
      },
      artifact_patterns: ['build/app/outputs/flutter-apk/app-debug.apk'],
    },
    trigger_config: {
      events: ['push'],
      branches: ['develop', 'feature/*'],
    },
    concurrency: { cancel_previous: true },
    enabled: true,
    created_at: ago(86400 * 40),
    updated_at: ago(86400 * 1),
  },
  {
    id: PIPELINE_IDS.paymentsAll,
    project_id: PROJECT_IDS.nativePayments,
    name: 'Multi-platform Release',
    config_path: '.oore/release.yaml',
    config_path_explicit: true,
    execution_config: {
      platforms: ['android', 'ios'],
      flutter_version: '3.24.3',
      commands: {
        pre_build: ['flutter pub get', 'flutter analyze'],
        build: [],
        post_build: ['flutter test'],
      },
      platform_commands: {
        android: 'flutter build apk --release',
        ios: 'flutter build ipa --release',
      },
      env: [{ key: 'PAYMENT_ENV', value: 'production' }],
      artifact_patterns: [
        'build/app/outputs/flutter-apk/*.apk',
        'build/ios/ipa/*.ipa',
      ],
    },
    trigger_config: {
      events: ['push'],
      branches: ['main'],
    },
    concurrency: { cancel_previous: false, max_concurrent: 1 },
    enabled: true,
    created_at: ago(86400 * 18),
    updated_at: ago(86400 * 2),
  },
]
