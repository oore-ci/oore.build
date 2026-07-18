import { HttpResponse, delay, http } from 'msw'
import { ago } from '../seed'
import type { Integration } from '@/lib/types'
import { requireDemoInstancePermission } from '../authorization'
import { demoState } from '../state'

export const integrationHandlers = [
  http.get('/v1/integrations', async ({ request }) => {
    await delay(150)
    const url = new URL(request.url)
    const provider = url.searchParams.get('provider')
    const integrations = provider
      ? demoState.integrations.filter((item) => item.provider === provider)
      : demoState.integrations
    const limit = Math.min(Number(url.searchParams.get('limit')) || 50, 200)
    const offset = Number(url.searchParams.get('offset')) || 0
    return HttpResponse.json({
      integrations: integrations.slice(offset, offset + limit),
      total: integrations.length,
    })
  }),

  http.get('/v1/integrations/:id', async ({ params }) => {
    await delay(150)
    const integration = demoState.integrations.find((i) => i.id === params.id)
    if (!integration) {
      return HttpResponse.json(
        { error: 'Integration not found', code: 'not_found' },
        { status: 404 },
      )
    }
    const installations = demoState.installations[integration.id] ?? []
    const repos = demoState.repositories[integration.id] ?? []
    return HttpResponse.json({
      integration,
      installation_count: installations.length,
      repository_count: repos.length,
      last_webhook_at: ago(3600),
    })
  }),

  http.delete('/v1/integrations/:id', async ({ params, request }) => {
    await delay(200)
    const forbidden = requireDemoInstancePermission(
      request,
      'integrations:delete',
    )
    if (forbidden) return forbidden
    const id = String(params.id)
    const repositoryIds = new Set(
      (demoState.repositories[id] ?? []).map((repository) => repository.id),
    )
    demoState.integrations = demoState.integrations.filter(
      (integration) => integration.id !== id,
    )
    delete demoState.installations[id]
    delete demoState.repositories[id]
    for (const project of demoState.projects) {
      if (project.repository_id && repositoryIds.has(project.repository_id)) {
        project.repository_id = undefined
        project.repository_full_name = undefined
        project.repository_avatar_url = undefined
      }
    }
    return HttpResponse.json({ ok: true })
  }),

  http.get('/v1/integrations/:id/repositories', async ({ params, request }) => {
    await delay(150)
    const integration = demoState.integrations.find(
      (candidate) => candidate.id === params.id,
    )
    if (demoState.scenario === 'degraded' && integration?.status === 'error') {
      return HttpResponse.json(
        {
          error: 'Source synchronization is unavailable.',
          code: 'source_error',
        },
        { status: 503 },
      )
    }
    const url = new URL(request.url)
    const limit = Math.min(Number(url.searchParams.get('limit')) || 500, 500)
    const offset = Math.max(Number(url.searchParams.get('offset')) || 0, 0)
    const repositories = demoState.repositories[params.id as string] ?? []
    return HttpResponse.json({
      repositories: repositories.slice(offset, offset + limit),
    })
  }),

  http.get(
    '/v1/integration-repositories/:repositoryId/avatar',
    async ({ params }) => {
      await delay(80)
      const repository = Object.values(demoState.repositories)
        .flatMap((repositories) => repositories ?? [])
        .find((item) => item.id === params.repositoryId)
      if (!repository) {
        return HttpResponse.json(
          { error: 'Repository not found', code: 'not_found' },
          { status: 404 },
        )
      }
      const initials = repository.full_name
        .split('/')
        .at(-1)!
        .replaceAll(/[^a-z0-9]/gi, '')
        .slice(0, 2)
        .toUpperCase()
      return new HttpResponse(
        `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" fill="#e24329"/><text x="32" y="39" text-anchor="middle" font-family="sans-serif" font-size="22" font-weight="700" fill="white">${initials}</text></svg>`,
        { headers: { 'Content-Type': 'image/svg+xml' } },
      )
    },
  ),

  http.put(
    '/v1/integration-repositories/:repositoryId/runner-policy',
    async ({ params, request }) => {
      await delay(200)
      const forbidden = requireDemoInstancePermission(
        request,
        'integrations:write',
      )
      if (forbidden) return forbidden
      const repository = Object.values(demoState.repositories)
        .flatMap((repositories) => repositories ?? [])
        .find((item) => item.id === params.repositoryId)
      if (!repository) {
        return HttpResponse.json(
          { error: 'Repository not found', code: 'not_found' },
          { status: 404 },
        )
      }
      const body = (await request.json()) as {
        allow_direct_macos_runner?: boolean
      }
      repository.allow_direct_macos_runner =
        body.allow_direct_macos_runner === true
      return HttpResponse.json({ repository })
    },
  ),

  http.get('/v1/integrations/:id/installations', async ({ params }) => {
    await delay(150)
    return HttpResponse.json({
      installations: demoState.installations[params.id as string] ?? [],
    })
  }),

  http.post(
    '/v1/integrations/:id/installations',
    async ({ params, request }) => {
      await delay(300)
      const forbidden = requireDemoInstancePermission(
        request,
        'integrations:write',
      )
      if (forbidden) return forbidden
      return HttpResponse.json({
        installations: demoState.installations[params.id as string] ?? [],
      })
    },
  ),

  // GitHub App creation — return a no-op URL
  http.post('/v1/integrations/github/start', async ({ request }) => {
    await delay(200)
    const forbidden = requireDemoInstancePermission(
      request,
      'integrations:write',
    )
    if (forbidden) return forbidden
    return HttpResponse.json({ create_url: '#demo-github-app' })
  }),

  http.post('/v1/integrations/github/complete', async ({ request }) => {
    await delay(300)
    const forbidden = requireDemoInstancePermission(
      request,
      'integrations:write',
    )
    if (forbidden) return forbidden
    return HttpResponse.json({ integration: demoState.integrations[0] })
  }),

  // GitLab integration
  http.post('/v1/integrations/gitlab/start', async ({ request }) => {
    await delay(300)
    const forbidden = requireDemoInstancePermission(
      request,
      'integrations:write',
    )
    if (forbidden) return forbidden
    return HttpResponse.json({ integration: demoState.integrations[1] })
  }),

  http.post('/v1/integrations/gitlab/authorize', async ({ request }) => {
    await delay(200)
    const forbidden = requireDemoInstancePermission(
      request,
      'integrations:write',
    )
    if (forbidden) return forbidden
    return HttpResponse.json({ authorize_url: '#demo-gitlab-auth' })
  }),

  http.get('/v1/integrations/local-git', async () => {
    await delay(120)
    return HttpResponse.json({
      integrations: demoState.integrations.filter(
        (integration) => integration.provider === 'local_git',
      ),
      total: demoState.integrations.filter(
        (integration) => integration.provider === 'local_git',
      ).length,
    })
  }),

  http.post('/v1/integrations/local-git', async ({ request }) => {
    await delay(200)
    const forbidden = requireDemoInstancePermission(
      request,
      'integrations:write',
    )
    if (forbidden) return forbidden
    const payload = (await request.json()) as {
      repository_path?: string
      display_name?: string
    }
    const now = Math.floor(Date.now() / 1000)
    const index =
      demoState.integrations.filter(
        (candidate) => candidate.provider === 'local_git',
      ).length + 1
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
    demoState.integrations.unshift(integration)
    const repository = {
      id: `repo-demo-local-${index.toString().padStart(3, '0')}`,
      installation_id: '',
      external_id: payload.repository_path ?? '/tmp/demo-repo',
      full_name: (payload.repository_path ?? 'demo-repo').split('/').pop()!,
      is_private: true,
      allow_direct_macos_runner: false,
      created_at: now,
      updated_at: now,
    }
    demoState.repositories[integration.id] = [repository]
    demoState.installations[integration.id] = []

    return HttpResponse.json({
      integration,
      repository,
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

  http.delete('/v1/integrations/local-git/:id', async ({ params, request }) => {
    await delay(120)
    const forbidden = requireDemoInstancePermission(
      request,
      'integrations:delete',
    )
    if (forbidden) return forbidden
    const id = String(params.id)
    demoState.integrations = demoState.integrations.filter(
      (item) => item.id !== id,
    )
    delete demoState.repositories[id]
    delete demoState.installations[id]
    return HttpResponse.json({ ok: true })
  }),
]
