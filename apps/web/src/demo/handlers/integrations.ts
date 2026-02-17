import { HttpResponse, delay, http } from 'msw'
import {
  demoInstallations,
  demoIntegrations,
  demoRepositories,
} from '../data/integrations'
import { ago } from '../seed'
import type { Integration } from '@/lib/types'

const localGitIntegrations: Array<Integration> = []

export const integrationHandlers = [
  http.get('/v1/integrations', async ({ request }) => {
    await delay(150)
    const url = new URL(request.url)
    const provider = url.searchParams.get('provider')
    const combined = [...localGitIntegrations, ...demoIntegrations]
    const integrations = provider
      ? combined.filter((item) => item.provider === provider)
      : combined
    return HttpResponse.json({
      integrations,
      total: integrations.length,
    })
  }),

  http.get('/v1/integrations/:id', async ({ params }) => {
    await delay(150)
    const integration = [...localGitIntegrations, ...demoIntegrations].find(
      (i) => i.id === params.id,
    )
    if (!integration) {
      return HttpResponse.json(
        { error: 'Integration not found', code: 'not_found' },
        { status: 404 },
      )
    }
    const installations = demoInstallations[integration.id] ?? []
    const repos = demoRepositories[integration.id] ?? []
    return HttpResponse.json({
      integration,
      installation_count: installations.length,
      repository_count: repos.length,
      last_webhook_at: ago(3600),
    })
  }),

  http.delete('/v1/integrations/:id', async () => {
    await delay(200)
    return HttpResponse.json({ ok: true })
  }),

  http.get('/v1/integrations/:id/repositories', async ({ params }) => {
    await delay(150)
    return HttpResponse.json({
      repositories: demoRepositories[params.id as string] ?? [],
    })
  }),

  http.get('/v1/integrations/:id/installations', async ({ params }) => {
    await delay(150)
    return HttpResponse.json({
      installations: demoInstallations[params.id as string] ?? [],
    })
  }),

  http.post('/v1/integrations/:id/installations', async ({ params }) => {
    await delay(300)
    return HttpResponse.json({
      installations: demoInstallations[params.id as string] ?? [],
    })
  }),

  // GitHub App creation — return a no-op URL
  http.post('/v1/integrations/github/start', async () => {
    await delay(200)
    return HttpResponse.json({ create_url: '#demo-github-app' })
  }),

  http.post('/v1/integrations/github/complete', async () => {
    await delay(300)
    return HttpResponse.json({ integration: demoIntegrations[0] })
  }),

  // GitLab integration
  http.post('/v1/integrations/gitlab/start', async () => {
    await delay(300)
    return HttpResponse.json({ integration: demoIntegrations[1] })
  }),

  http.post('/v1/integrations/gitlab/authorize', async () => {
    await delay(200)
    return HttpResponse.json({ authorize_url: '#demo-gitlab-auth' })
  }),

  http.get('/v1/integrations/local-git', async () => {
    await delay(120)
    return HttpResponse.json({
      integrations: localGitIntegrations,
      total: localGitIntegrations.length,
    })
  }),

  http.post('/v1/integrations/local-git', async ({ request }) => {
    await delay(200)
    const payload = (await request.json()) as {
      repository_path?: string
      display_name?: string
    }
    const now = Math.floor(Date.now() / 1000)
    const index = localGitIntegrations.length + 1
    const integration: Integration = {
      id: `integ-demo-local-${index.toString().padStart(3, '0')}`,
      provider: 'local_git',
      host_url: 'local://filesystem',
      auth_mode: 'local_path',
      status: 'active',
      display_name: payload.display_name || `local-repo-${index}`,
      created_by: 'usr-demo-owner-001',
      created_at: now,
      updated_at: now,
    }
    localGitIntegrations.unshift(integration)

    return HttpResponse.json({
      integration,
      repository: {
        id: `repo-demo-local-${index.toString().padStart(3, '0')}`,
        installation_id: `install-demo-local-${index.toString().padStart(3, '0')}`,
        external_id: payload.repository_path ?? '/tmp/demo-repo',
        full_name: (payload.repository_path ?? 'demo-repo').split('/').pop(),
        is_private: true,
        created_at: now,
        updated_at: now,
      },
    })
  }),

  http.get('/v1/integrations/local-git/directories', async ({ request }) => {
    await delay(120)
    const url = new URL(request.url)
    const currentPath = url.searchParams.get('path') ?? '/Users/demo'
    const suggestions = [
      { label: 'Home', path: '/Users/demo' },
      { label: 'Desktop', path: '/Users/demo/Desktop' },
      { label: 'Documents', path: '/Users/demo/Documents' },
      { label: 'Downloads', path: '/Users/demo/Downloads' },
      { label: 'Code', path: '/Users/demo/Code' },
    ]

    return HttpResponse.json({
      current_path: currentPath,
      current_is_git_repository: currentPath.endsWith('demo-repo'),
      parent_path:
        currentPath === '/Users/demo'
          ? '/Users'
          : currentPath.split('/').slice(0, -1).join('/') || '/',
      suggestions,
      directories: [
        {
          name: 'demo-repo',
          path: `${currentPath}/demo-repo`,
          is_git_repository: true,
        },
        {
          name: 'mobile-app',
          path: `${currentPath}/mobile-app`,
          is_git_repository: true,
        },
        {
          name: 'playground',
          path: `${currentPath}/playground`,
          is_git_repository: false,
        },
      ],
    })
  }),

  http.delete('/v1/integrations/local-git/:id', async ({ params }) => {
    await delay(120)
    const index = localGitIntegrations.findIndex(
      (item) => item.id === params.id,
    )
    if (index >= 0) {
      localGitIntegrations.splice(index, 1)
    }
    return HttpResponse.json({ ok: true })
  }),
]
