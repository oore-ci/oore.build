import { INTEGRATION_IDS, USER_IDS, ago } from '../seed'
import type {
  Integration,
  IntegrationInstallation,
  IntegrationRepository,
} from '@/lib/types'

export const demoIntegrations: Array<Integration> = [
  {
    id: INTEGRATION_IDS.github,
    provider: 'github',
    host_url: 'https://github.com',
    auth_mode: 'github_app',
    status: 'active',
    display_name: 'oore-ci',
    app_id: '12345',
    app_slug: 'oore-ci',
    created_by: USER_IDS.owner,
    created_at: ago(86400 * 60),
    updated_at: ago(86400 * 1),
  },
  {
    id: INTEGRATION_IDS.gitlab,
    provider: 'gitlab',
    host_url: 'https://gitlab.com',
    auth_mode: 'oauth_app',
    status: 'active',
    display_name: 'GitLab CI Integration',
    created_by: USER_IDS.admin,
    created_at: ago(86400 * 30),
    updated_at: ago(86400 * 5),
  },
]

export const demoInstallations: Record<
  string,
  Array<IntegrationInstallation>
> = {
  [INTEGRATION_IDS.github]: [
    {
      id: 'install-001',
      integration_id: INTEGRATION_IDS.github,
      external_id: '98765',
      account_name: 'acme-corp',
      account_type: 'Organization',
      created_at: ago(86400 * 58),
    },
  ],
  [INTEGRATION_IDS.gitlab]: [],
}

export const demoRepositories: Record<string, Array<IntegrationRepository>> = {
  [INTEGRATION_IDS.github]: [
    {
      id: 'repo-001',
      installation_id: 'install-001',
      external_id: '111',
      full_name: 'acme-corp/flutter-shop',
      default_branch: 'main',
      is_private: true,
      allow_direct_macos_runner: true,
      avatar_url: 'https://avatars.githubusercontent.com/u/9919?v=4',
      created_at: ago(86400 * 58),
      updated_at: ago(86400 * 1),
    },
    {
      id: 'repo-002',
      installation_id: 'install-001',
      external_id: '222',
      full_name: 'acme-corp/internal-admin',
      default_branch: 'develop',
      is_private: true,
      allow_direct_macos_runner: false,
      avatar_url: 'https://avatars.githubusercontent.com/u/9919?v=4',
      created_at: ago(86400 * 45),
      updated_at: ago(86400 * 2),
    },
    {
      id: 'repo-003-gh',
      installation_id: 'install-001',
      external_id: '333',
      full_name: 'acme-corp/design-system',
      default_branch: 'main',
      is_private: false,
      allow_direct_macos_runner: true,
      avatar_url: 'https://avatars.githubusercontent.com/u/9919?v=4',
      created_at: ago(86400 * 30),
      updated_at: ago(86400 * 10),
    },
  ],
  [INTEGRATION_IDS.gitlab]: [
    {
      id: 'repo-003',
      installation_id: '',
      external_id: '444',
      full_name: 'acme-corp/native-payments',
      default_branch: 'main',
      is_private: true,
      allow_direct_macos_runner: true,
      avatar_url:
        'https://gitlab.com/uploads/-/system/group/avatar/6543/gitlab-logo-500.png',
      created_at: ago(86400 * 25),
      updated_at: ago(86400 * 3),
    },
  ],
}
