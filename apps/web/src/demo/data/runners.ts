import { RUNNER_IDS, USER_IDS, ago } from '../seed'
import type { Runner } from '@/lib/types'

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
    },
    last_heartbeat_at: ago(3600 * 2),
    registered_by: USER_IDS.owner,
    created_at: ago(86400 * 60),
    updated_at: ago(3600 * 2),
  },
]
