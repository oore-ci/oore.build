import { INTEGRATION_IDS, PROJECT_IDS, USER_IDS, ago } from '../seed'
import type { Project } from '@/lib/types'

export const GITHUB_DEMO_AVATAR_URL = 'https://github.com/github.png'
export const GITLAB_DEMO_AVATAR_URL =
  'https://about.gitlab.com/images/press/gitlab-logo-500-rgb.png'

export const demoProjects: Array<Project> = [
  {
    id: PROJECT_IDS.flutterShop,
    name: 'FlutterShop',
    description: 'E-commerce mobile app — Android & iOS',
    repository_id: `${INTEGRATION_IDS.github}:repo-001`,
    repository_full_name: 'acme-corp/flutter-shop',
    repository_avatar_url: GITHUB_DEMO_AVATAR_URL,
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
    repository_full_name: 'acme-corp/internal-admin',
    repository_avatar_url: GITHUB_DEMO_AVATAR_URL,
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
    repository_full_name: 'acme-corp/native-payments',
    repository_avatar_url: GITLAB_DEMO_AVATAR_URL,
    default_branch: 'main',
    settings: {},
    created_by: USER_IDS.owner,
    created_at: ago(86400 * 20),
    updated_at: ago(86400 * 2),
  },
]
