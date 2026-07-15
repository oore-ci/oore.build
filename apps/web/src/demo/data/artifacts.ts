import { BUILD_IDS, ago } from '../seed'
import type { Artifact } from '@/lib/types'

export const demoArtifacts: Record<string, Array<Artifact>> = {
  [BUILD_IDS.succeeded1]: [
    {
      id: 'art-001',
      build_id: BUILD_IDS.succeeded1,
      name: 'app-armeabi-v7a-release.apk',
      artifact_type: 'apk',
      file_path: 'build/app/outputs/flutter-apk/app-armeabi-v7a-release.apk',
      file_size: 19084288,
      checksum: 'sha256:a1b2c3d4e5f6...',
      metadata: { abi: 'armeabi-v7a', minSdk: 21 },
      created_at: ago(7020),
    },
    {
      id: 'art-002',
      build_id: BUILD_IDS.succeeded1,
      name: 'app-arm64-v8a-release.apk',
      artifact_type: 'apk',
      file_path: 'build/app/outputs/flutter-apk/app-arm64-v8a-release.apk',
      file_size: 20054016,
      checksum: 'sha256:b2c3d4e5f6a7...',
      metadata: { abi: 'arm64-v8a', minSdk: 21 },
      created_at: ago(7020),
    },
  ],
  [BUILD_IDS.succeeded2]: [
    {
      id: 'art-003',
      build_id: BUILD_IDS.succeeded2,
      name: 'app-debug.apk',
      artifact_type: 'apk',
      file_path: 'build/app/outputs/flutter-apk/app-debug.apk',
      file_size: 44695552,
      metadata: { buildType: 'debug' },
      created_at: ago(14160),
    },
  ],
  [BUILD_IDS.succeeded3]: [
    {
      id: 'art-004',
      build_id: BUILD_IDS.succeeded3,
      name: 'FlutterShop.ipa',
      artifact_type: 'ipa',
      file_path: 'build/ios/ipa/FlutterShop.ipa',
      file_size: 32505856,
      checksum: 'sha256:c3d4e5f6a7b8...',
      metadata: {
        ios_app: {
          bundle_identifier: 'build.oore.fluttershop',
          display_name: 'Flutter Shop',
          version: '1.4.0',
          build_number: '84',
        },
        ios_signing: {
          bundle_ids: ['build.oore.fluttershop'],
          effective_export_method: 'release-testing',
        },
      },
      created_at: ago(28260),
    },
  ],
}
