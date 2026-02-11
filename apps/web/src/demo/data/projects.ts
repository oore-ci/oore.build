import { INTEGRATION_IDS, PROJECT_IDS, USER_IDS, ago } from '../seed'
import type { Project } from '@/lib/types'

export const demoProjects: Array<Project> = [
  {
    id: PROJECT_IDS.flutterShop,
    name: 'FlutterShop',
    description: 'E-commerce mobile app — Android & iOS',
    repository_id: `${INTEGRATION_IDS.github}:repo-001`,
    default_branch: 'main',
    settings: {},
    created_by: USER_IDS.owner,
    created_at: ago(86400 * 60),
    updated_at: ago(3600 * 3),
  },
  {
    id: PROJECT_IDS.internalAdmin,
    name: 'InternalAdmin',
    description: 'Internal admin dashboard for ops team',
    repository_id: `${INTEGRATION_IDS.github}:repo-002`,
    default_branch: 'develop',
    settings: {},
    created_by: USER_IDS.admin,
    created_at: ago(86400 * 45),
    updated_at: ago(86400 * 1),
  },
  {
    id: PROJECT_IDS.nativePayments,
    name: 'NativePayments',
    description: 'Payment SDK with platform-specific native code',
    repository_id: `${INTEGRATION_IDS.gitlab}:repo-003`,
    default_branch: 'main',
    settings: {},
    created_by: USER_IDS.owner,
    created_at: ago(86400 * 20),
    updated_at: ago(86400 * 2),
  },
]
