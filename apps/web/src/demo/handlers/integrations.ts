import { HttpResponse, delay, http } from 'msw'
import {
  demoInstallations,
  demoIntegrations,
  demoRepositories,
} from '../data/integrations'
import { ago } from '../seed'

export const integrationHandlers = [
  http.get('/v1/integrations', async () => {
    await delay(150)
    return HttpResponse.json({
      integrations: demoIntegrations,
      total: demoIntegrations.length,
    })
  }),

  http.get('/v1/integrations/:id', async ({ params }) => {
    await delay(150)
    const integration = demoIntegrations.find((i) => i.id === params.id)
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
]
