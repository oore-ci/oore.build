import { ago } from '../seed'
import type { ArtifactStorageSettings, InstancePreferences } from '@/lib/types'

export const demoArtifactStorageSettings: ArtifactStorageSettings = {
  provider: 'local',
  local_base_dir: '/var/oore/artifacts',
  has_access_key_id: false,
  has_secret_access_key: false,
  source: 'database',
  updated_at: ago(86400 * 30),
}

export const demoInstancePreferences: InstancePreferences = {
  key_storage_mode: 'keychain',
  runtime_mode: 'remote',
  remote_auth_mode: 'oidc',
  restart_required: false,
  direct_macos_runner_enabled: true,
  updated_at: ago(86400 * 60),
}
