import { RUNNER_IDS, USER_IDS, ago } from '../seed'
import type { Runner } from '@oore/client/models'

const GIB = 1024 * 1024 * 1024

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
    labels: ['arm64', 'macos', 'macos-15', 'release', 'xcode-16'],
    custom_labels: ['release'],
    metrics: {
      cpu_percent: 42,
      memory_used_bytes: 38 * GIB,
      memory_total_bytes: 64 * GIB,
      disk_free_bytes: 412 * GIB,
      disk_total_bytes: 1862 * GIB,
      cache_bytes: 96 * GIB,
      collected_at: ago(15),
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
    labels: ['arm64', 'macos', 'macos-15', 'xcode-16'],
    custom_labels: [],
    last_heartbeat_at: ago(3600 * 2),
    registered_by: USER_IDS.owner,
    created_at: ago(86400 * 60),
    updated_at: ago(3600 * 2),
  },
]
