import { demoApi } from './api'
import { HttpResponse, delay } from 'msw'
import * as z from 'zod'
import { ago } from '../seed'
import type { Integration } from '@oore/client/models'
import { requireDemoInstancePermission } from '../authorization'
import { demoState } from '../state'

export const integrationHandlers = [
  demoApi.listOperatorIncidents(async ({ request }) => {
    await delay(80)
    const resourceId = new URL(request.url).searchParams.get('resource_id')
    const source = demoState.integrations.find(
      (integration) =>
        integration.provider === 'gitlab' &&
        integration.status === 'error' &&
        (!resourceId || integration.id === resourceId),
    )
    if (!source) return HttpResponse.json({ incidents: [] })
    return HttpResponse.json({
      incidents: [
        {
          id: `incident-${source.id}`,
          status: 'open',
          severity: 'critical',
          reason: 'rejected',
          first_occurrence_at: ago(7200),
          latest_occurrence_at: ago(600),
          occurrence_count: 3,
          resource_kind: 'source',
          resource_id: source.id,
          resource_name: source.display_name ?? 'GitLab source',
          repair_action: 'Reconnect GitLab',
          repair_url: `/settings/sources/${source.id}?tab=connection`,
        },
      ],
    })
  }),

  demoApi.markOperatorIncidentRead(async () => {
    await delay(40)
    return HttpResponse.json({ ok: true })
  }),

  demoApi.listIntegrations(async ({ request }) => {
    await delay(150)
    const url = new URL(request.url)
    const provider = url.searchParams.get('provider')
    const query = url.searchParams.get('q')?.toLowerCase()
    const sort = url.searchParams.get('sort') ?? 'updated_at'
    const direction = url.searchParams.get('direction') === 'asc' ? 1 : -1
    const integrations = (
      provider
        ? demoState.integrations.filter((item) => item.provider === provider)
        : demoState.integrations
    )
      .filter((integration) =>
        query
          ? [
              integration.display_name,
              integration.provider,
              integration.host_url,
              integration.auth_mode,
              integration.status,
            ]
              .filter(Boolean)
              .some((value) => value?.toLowerCase().includes(query))
          : true,
      )
      .slice()
      .sort((left, right) => {
        const result =
          sort === 'name'
            ? (left.display_name ?? left.provider).localeCompare(
                right.display_name ?? right.provider,
              )
            : sort === 'provider'
              ? left.provider.localeCompare(right.provider)
              : sort === 'status'
                ? left.status.localeCompare(right.status)
                : left.updated_at - right.updated_at
        return direction * result
      })
    const limit = Math.min(Number(url.searchParams.get('limit')) || 20, 200)
    const offset = Number(url.searchParams.get('offset')) || 0
    return HttpResponse.json({
      integrations: integrations.slice(offset, offset + limit),
      total: integrations.length,
      active_total: demoState.integrations.filter(
        (integration) => integration.status === 'active',
      ).length,
    })
  }),

  demoApi.getIntegration(async ({ params }) => {
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

  demoApi.deleteIntegration(async ({ params, request }) => {
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

  demoApi.listRepositories(async ({ params, request }) => {
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
    const repositories = demoState.repositories[String(params.id)] ?? []
    const query = url.searchParams.get('q')?.toLowerCase()
    const filtered = repositories.filter((repository) =>
      query
        ? [
            repository.full_name,
            repository.default_branch,
            repository.is_private ? 'private' : 'public',
          ]
            .filter(Boolean)
            .some((value) => value?.toLowerCase().includes(query))
        : true,
    )
    return HttpResponse.json({
      repositories: filtered.slice(offset, offset + limit),
      total: filtered.length,
    })
  }),

  demoApi.listSourceRepositories(async ({ request }) => {
    await delay(150)
    const url = new URL(request.url)
    const query = url.searchParams.get('q')?.toLowerCase()
    const limit = Math.min(Number(url.searchParams.get('limit')) || 100, 100)
    const offset = Math.max(Number(url.searchParams.get('offset')) || 0, 0)
    const repositories = Object.entries(demoState.repositories)
      .flatMap(([integrationId, repositories]) => {
        const integration = demoState.integrations.find(
          (candidate) => candidate.id === integrationId,
        )
        if (!integration) return []
        return (repositories ?? []).map((repository) => ({
          ...repository,
          integration_id: integration.id,
          provider: integration.provider,
          host_url: integration.host_url,
        }))
      })
      .filter((repository) =>
        query
          ? [
              repository.full_name,
              repository.default_branch,
              repository.provider,
              repository.host_url,
            ]
              .filter(Boolean)
              .some((value) => value?.toLowerCase().includes(query))
          : true,
      )
      .slice()
      .sort((left, right) => left.full_name.localeCompare(right.full_name))
    return HttpResponse.json({
      repositories: repositories.slice(offset, offset + limit),
      total: repositories.length,
    })
  }),

  demoApi.repositoryAvatar(async ({ params }) => {
    await delay(80)
    const repository = Object.values(demoState.repositories)
      .flatMap((repositories) => repositories ?? [])
      .find((item) => item.id === params.id)
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
  }),

  demoApi.listInstallations(async ({ params }) => {
    await delay(150)
    return HttpResponse.json({
      installations: demoState.installations[String(params.id)] ?? [],
    })
  }),

  demoApi.syncInstallations(async ({ params, request }) => {
    await delay(300)
    const forbidden = requireDemoInstancePermission(
      request,
      'integrations:write',
    )
    if (forbidden) return forbidden
    return HttpResponse.json({
      installations: demoState.installations[String(params.id)] ?? [],
    })
  }),

  // GitHub App creation — return a no-op URL
  demoApi.githubStart(async ({ request }) => {
    await delay(200)
    const forbidden = requireDemoInstancePermission(
      request,
      'integrations:write',
    )
    if (forbidden) return forbidden
    return HttpResponse.json({ create_url: '#demo-github-app' })
  }),

  demoApi.githubComplete(async ({ request }) => {
    await delay(300)
    const forbidden = requireDemoInstancePermission(
      request,
      'integrations:write',
    )
    if (forbidden) return forbidden
    return HttpResponse.json({ integration: demoState.integrations[0] })
  }),

  // GitLab integration
  demoApi.gitlabStart(async ({ request }) => {
    await delay(300)
    const forbidden = requireDemoInstancePermission(
      request,
      'integrations:write',
    )
    if (forbidden) return forbidden
    return HttpResponse.json({ integration: demoState.integrations[1] })
  }),

  demoApi.gitlabAuthorize(async ({ request }) => {
    await delay(200)
    const forbidden = requireDemoInstancePermission(
      request,
      'integrations:write',
    )
    if (forbidden) return forbidden
    return HttpResponse.json({ authorize_url: '#demo-gitlab-auth' })
  }),

  demoApi.listLocalGitIntegrations(async () => {
    await delay(120)
    return HttpResponse.json({
      integrations: demoState.integrations.filter(
        (integration) => integration.provider === 'local_git',
      ),
      total: demoState.integrations.filter(
        (integration) => integration.provider === 'local_git',
      ).length,
      active_total: demoState.integrations.filter(
        (integration) =>
          integration.provider === 'local_git' &&
          integration.status === 'active',
      ).length,
    })
  }),

  demoApi.createLocalGitIntegration(async ({ request }) => {
    await delay(200)
    const forbidden = requireDemoInstancePermission(
      request,
      'integrations:write',
    )
    if (forbidden) return forbidden
    const payload = z
      .object({
        repository_path: z.string().optional(),
        display_name: z.string().optional(),
      })
      .parse(await request.json())
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

  demoApi.browseLocalGitDirectories(async ({ request }) => {
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

  demoApi.deleteLocalGitIntegration(async ({ params, request }) => {
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
