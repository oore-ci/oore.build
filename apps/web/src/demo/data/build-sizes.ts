import { BUILD_IDS } from '../seed'
import type { BuildSizeBaseline } from '@oore/client/models'

interface DemoSizeBaseline {
  baseline: BuildSizeBaseline
  /** Artifact sizes of the baseline build, by name. */
  sizes: ReadonlyMap<string, number>
}

/** Baselines for builds whose artifacts show a size change in the demo. */
export const demoSizeBaselines = new Map<string, DemoSizeBaseline>([
  [
    BUILD_IDS.succeeded1,
    {
      baseline: {
        build_id: BUILD_IDS.succeeded4,
        build_number: 138,
        branch: 'main',
      },
      sizes: new Map([
        ['app-armeabi-v7a-release.apk', 17_825_792],
        ['app-arm64-v8a-release.apk', 19_922_944],
        ['FlutterShop.app', 73_400_320],
        ['coverage-report.zip', 1_572_864],
      ]),
    },
  ],
])
