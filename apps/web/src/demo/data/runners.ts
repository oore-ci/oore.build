import { RUNNER_IDS, USER_IDS, ago } from '../seed'
import type { Runner } from '@oore/client/models'

export const demoRunners: Array<Runner> = [
  {
    id: RUNNER_IDS.macStudio,
    name: 'mac-studio-m2-ultra',
    status: 'online',
    capabilities: {
      os: 'macOS',
      arch: 'arm64',
      chip: 'M2 Ultra',
      memory_gb: 64,
      xcode: '15.4',
      flutter: '3.24.3',
      android_sdk: '34',
      caches: {
        total_bytes: 19756849561,
        max_bytes: 53687091200,
        measured_at: ago(600),
        evicted_bytes: 0,
        kinds: [
          { name: 'pub', bytes: 2254857830 },
          { name: 'gradle', bytes: 10307921510 },
          { name: 'cocoapods', bytes: 1288490188 },
          { name: 'cocoapods-specs', bytes: 966367641 },
          { name: 'derived-data', bytes: 4939212390 },
        ],
      },
    },
    last_heartbeat_at: ago(15),
    registered_by: USER_IDS.owner,
    created_at: ago(86400 * 90),
    updated_at: ago(15),
  },
  {
    id: RUNNER_IDS.macMini,
    name: 'mac-mini-m2',
    status: 'offline',
    capabilities: {
      os: 'macOS',
      arch: 'arm64',
      chip: 'M2',
      memory_gb: 16,
      xcode: '15.4',
      flutter: '3.24.3',
      android_sdk: '34',
      caches: {
        total_bytes: 6764573491,
        max_bytes: 53687091200,
        measured_at: ago(3600 * 2),
        evicted_bytes: 0,
        kinds: [],
      },
    },
    last_heartbeat_at: ago(3600 * 2),
    registered_by: USER_IDS.owner,
    created_at: ago(86400 * 60),
    updated_at: ago(3600 * 2),
  },
]
