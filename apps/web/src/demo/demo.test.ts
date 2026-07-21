import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest'
import { setupServer } from 'msw/node'
import { allHandlers } from './handlers'
import {
  DEMO_PERSONAS,
  authenticateDemoUser,
  getDemoProjectRole,
} from './personas'
import {
  BUILD_IDS,
  DEMO_PASSWORD,
  INTEGRATION_IDS,
  NOTIFICATION_CHANNEL_IDS,
  PIPELINE_IDS,
  PROJECT_IDS,
  USER_IDS,
} from './seed'

const server = setupServer(...allHandlers)
const demoOrigin = window.location.origin

beforeAll(() => server.listen({ onUnhandledRequest: 'error' }))
afterEach(() => server.resetHandlers())
afterAll(() => server.close())

describe('demo personas', () => {
  it('accepts each documented account with the shared password', () => {
    for (const persona of DEMO_PERSONAS) {
      expect(authenticateDemoUser(persona.email, DEMO_PASSWORD)?.role).toBe(
        persona.role,
      )
    }
    expect(authenticateDemoUser(DEMO_PERSONAS[0].email, 'wrong')).toBeNull()
  })

  it('keeps project membership role-scoped', () => {
    const developer = DEMO_PERSONAS.find(
      (persona) => persona.role === 'developer',
    )!
    expect(getDemoProjectRole(developer, PROJECT_IDS.flutterShop)).toBe(
      'maintainer',
    )
    expect(getDemoProjectRole(developer, PROJECT_IDS.nativePayments)).toBe(
      'viewer',
    )
    expect(getDemoProjectRole(developer, PROJECT_IDS.internalAdmin)).toBeNull()
  })
})

describe('demo API visibility', () => {
  it.each([
    ['owner', 3],
    ['admin', 3],
    ['developer', 2],
    ['qa_viewer', 2],
  ] as const)('%s sees only assigned projects', async (role, count) => {
    const persona = DEMO_PERSONAS.find((candidate) => candidate.role === role)!
    const response = await fetch(`${demoOrigin}/v1/projects`, {
      headers: { Authorization: `Bearer ${persona.token}` },
    })
    const body = (await response.json()) as {
      projects: Array<{ current_user_role?: string }>
      total: number
    }

    expect(response.headers.get('content-type')).toContain('application/json')
    expect(body.total).toBe(count)
    expect(body.projects).toHaveLength(count)
    expect(body.projects.every((project) => project.current_user_role)).toBe(
      true,
    )
  })

  it('returns JSON 404 for an unassigned project', async () => {
    const developer = DEMO_PERSONAS.find(
      (persona) => persona.role === 'developer',
    )!
    const response = await fetch(
      `${demoOrigin}/v1/projects/${PROJECT_IDS.internalAdmin}`,
      { headers: { Authorization: `Bearer ${developer.token}` } },
    )

    expect(response.status).toBe(404)
    expect(response.headers.get('content-type')).toContain('application/json')
    await expect(response.json()).resolves.toMatchObject({ code: 'not_found' })
  })

  it('rejects admin-only inventory for a developer', async () => {
    const developer = DEMO_PERSONAS.find(
      (persona) => persona.role === 'developer',
    )!
    const response = await fetch(`${demoOrigin}/v1/users`, {
      headers: { Authorization: `Bearer ${developer.token}` },
    })

    expect(response.status).toBe(403)
    await expect(response.json()).resolves.toMatchObject({ code: 'forbidden' })
  })

  it('keeps instance preferences admin-only for a developer', async () => {
    const developer = DEMO_PERSONAS.find(
      (persona) => persona.role === 'developer',
    )!
    const response = await fetch(`${demoOrigin}/v1/settings/preferences`, {
      headers: { Authorization: `Bearer ${developer.token}` },
    })

    expect(response.status).toBe(403)
    await expect(response.json()).resolves.toMatchObject({ code: 'forbidden' })
  })

  it('scopes eligible member candidates to project maintainers', async () => {
    const developer = DEMO_PERSONAS.find(
      (persona) => persona.role === 'developer',
    )!
    const headers = { Authorization: `Bearer ${developer.token}` }
    const [maintainerResponse, viewerResponse] = await Promise.all([
      fetch(
        `${demoOrigin}/v1/projects/${PROJECT_IDS.flutterShop}/members/candidates`,
        { headers },
      ),
      fetch(
        `${demoOrigin}/v1/projects/${PROJECT_IDS.nativePayments}/members/candidates`,
        { headers },
      ),
    ])

    expect(maintainerResponse.status).toBe(200)
    await expect(maintainerResponse.json()).resolves.toEqual({
      candidates: [
        expect.objectContaining({ id: USER_IDS.invited, role: 'developer' }),
      ],
    })
    expect(viewerResponse.status).toBe(403)
  })

  it('enforces instance and project roles for demo mutations', async () => {
    const developer = DEMO_PERSONAS.find(
      (persona) => persona.role === 'developer',
    )!
    const headers = {
      Authorization: `Bearer ${developer.token}`,
      'Content-Type': 'application/json',
    }

    const [
      invite,
      createProject,
      updateViewerProject,
      relinkMaintainerProject,
      runDeveloperProject,
    ] = await Promise.all([
      fetch(`${demoOrigin}/v1/users/invite`, {
        method: 'POST',
        headers,
        body: JSON.stringify({
          email: 'demo+new@oore.build',
          role: 'developer',
        }),
      }),
      fetch(`${demoOrigin}/v1/projects`, {
        method: 'POST',
        headers,
        body: JSON.stringify({ name: 'Developer project' }),
      }),
      fetch(`${demoOrigin}/v1/projects/${PROJECT_IDS.nativePayments}`, {
        method: 'PATCH',
        headers,
        body: JSON.stringify({ name: 'Should not change' }),
      }),
      fetch(`${demoOrigin}/v1/projects/${PROJECT_IDS.flutterShop}`, {
        method: 'PATCH',
        headers,
        body: JSON.stringify({ repository_id: 'repo-other' }),
      }),
      fetch(`${demoOrigin}/v1/projects/${PROJECT_IDS.flutterShop}/builds`, {
        method: 'POST',
        headers,
        body: JSON.stringify({ pipeline_id: PIPELINE_IDS.shopAndroid }),
      }),
    ])

    expect(invite.status).toBe(403)
    expect(createProject.status).toBe(403)
    await expect(createProject.json()).resolves.toMatchObject({
      code: 'forbidden',
    })
    expect(updateViewerProject.status).toBe(403)
    expect(relinkMaintainerProject.status).toBe(403)
    expect(runDeveloperProject.status).toBe(200)

    const runViewerProject = await fetch(
      `${demoOrigin}/v1/projects/${PROJECT_IDS.nativePayments}/builds`,
      {
        method: 'POST',
        headers,
        body: JSON.stringify({ pipeline_id: PIPELINE_IDS.paymentsAll }),
      },
    )
    expect(runViewerProject.status).toBe(403)
    await expect(runViewerProject.json()).resolves.toMatchObject({
      code: 'forbidden',
    })

    const [
      deleteMaintainerProject,
      deleteViewerProject,
      deleteMaintainerPipeline,
      deleteViewerPipeline,
      manageMaintainerMembers,
      manageViewerMembers,
    ] = await Promise.all([
      fetch(`${demoOrigin}/v1/projects/${PROJECT_IDS.flutterShop}`, {
        method: 'DELETE',
        headers,
      }),
      fetch(`${demoOrigin}/v1/projects/${PROJECT_IDS.nativePayments}`, {
        method: 'DELETE',
        headers,
      }),
      fetch(`${demoOrigin}/v1/pipelines/${PIPELINE_IDS.shopAndroid}`, {
        method: 'DELETE',
        headers,
      }),
      fetch(`${demoOrigin}/v1/pipelines/${PIPELINE_IDS.paymentsAll}`, {
        method: 'DELETE',
        headers,
      }),
      fetch(
        `${demoOrigin}/v1/projects/${PROJECT_IDS.flutterShop}/members/${USER_IDS.qaViewer}`,
        {
          method: 'PATCH',
          headers,
          body: JSON.stringify({ role: 'viewer' }),
        },
      ),
      fetch(
        `${demoOrigin}/v1/projects/${PROJECT_IDS.nativePayments}/members/${USER_IDS.qaViewer}`,
        {
          method: 'PATCH',
          headers,
          body: JSON.stringify({ role: 'viewer' }),
        },
      ),
    ])

    expect(deleteMaintainerProject.status).toBe(204)
    expect(deleteViewerProject.status).toBe(403)
    expect(deleteMaintainerPipeline.status).toBe(204)
    expect(deleteViewerPipeline.status).toBe(403)
    expect(manageMaintainerMembers.status).toBe(200)
    expect(manageViewerMembers.status).toBe(403)
  })

  it('lists API tokens with the backend role scope', async () => {
    const owner = DEMO_PERSONAS.find((persona) => persona.role === 'owner')!
    const developer = DEMO_PERSONAS.find(
      (persona) => persona.role === 'developer',
    )!

    const [ownerResponse, developerResponse] = await Promise.all([
      fetch(`${demoOrigin}/v1/api-tokens`, {
        headers: { Authorization: `Bearer ${owner.token}` },
      }),
      fetch(`${demoOrigin}/v1/api-tokens`, {
        headers: { Authorization: `Bearer ${developer.token}` },
      }),
    ])
    const ownerBody = (await ownerResponse.json()) as { tokens: Array<unknown> }
    const developerBody = (await developerResponse.json()) as {
      tokens: Array<{ created_by: string }>
    }

    expect(ownerResponse.status).toBe(200)
    expect(ownerBody.tokens).toHaveLength(5)
    expect(developerBody.tokens).toHaveLength(2)
    expect(
      developerBody.tokens.every(
        (token) => token.created_by === developer.userId,
      ),
    ).toBe(true)
  })

  it('mirrors collection search, sorting, pagination, and artifact limits', async () => {
    const owner = DEMO_PERSONAS.find((persona) => persona.role === 'owner')!
    const headers = { Authorization: `Bearer ${owner.token}` }
    const [
      projectsResponse,
      searchResponse,
      buildsResponse,
      auditResponse,
      artifactsResponse,
      integrationsResponse,
    ] = await Promise.all([
      fetch(
        `${demoOrigin}/v1/projects?sort=name&direction=desc&limit=1&offset=0`,
        { headers },
      ),
      fetch(`${demoOrigin}/v1/projects?search=payments`, { headers }),
      fetch(`${demoOrigin}/v1/builds?sort=status&direction=desc&limit=1`, {
        headers,
      }),
      fetch(`${demoOrigin}/v1/audit-logs?sort=action&direction=asc&limit=1`, {
        headers,
      }),
      fetch(
        `${demoOrigin}/v1/projects/${PROJECT_IDS.flutterShop}/artifacts?limit=1`,
        { headers },
      ),
      fetch(`${demoOrigin}/v1/integrations?limit=1&offset=1`, { headers }),
    ])
    const projects = (await projectsResponse.json()) as {
      projects: Array<{ name: string }>
      total: number
    }
    const search = (await searchResponse.json()) as {
      projects: Array<{ name: string }>
      total: number
    }
    const builds = (await buildsResponse.json()) as {
      builds: Array<{ status: string }>
      total: number
    }
    const audit = (await auditResponse.json()) as {
      entries: Array<{ action: string }>
      total: number
    }
    const artifacts = (await artifactsResponse.json()) as {
      artifacts: Array<unknown>
    }
    const integrations = (await integrationsResponse.json()) as {
      integrations: Array<unknown>
      total: number
    }

    expect(projects).toMatchObject({
      projects: [{ name: 'NativePayments' }],
      total: 3,
    })
    expect(search).toMatchObject({
      projects: [{ name: 'NativePayments' }],
      total: 1,
    })
    expect(builds.builds).toHaveLength(1)
    expect(builds.builds[0]?.status).toBe('timed_out')
    expect(builds.total).toBeGreaterThan(1)
    expect(audit.entries).toHaveLength(1)
    expect(audit.entries[0]?.action).toBe('cancel_build')
    expect(audit.total).toBeGreaterThan(1)
    expect(artifacts.artifacts).toHaveLength(1)
    expect(integrations.integrations).toHaveLength(1)
    expect(integrations.total).toBeGreaterThan(1)
  })

  it('serves every authenticated demo read surface without fallthrough', async () => {
    const owner = DEMO_PERSONAS.find((persona) => persona.role === 'owner')!
    const paths = [
      '/healthz',
      '/__oore_web_healthz',
      '/__oore_web_update',
      '/v1/system/update',
      '/v1/users/me',
      '/v1/users',
      '/v1/projects',
      `/v1/projects/${PROJECT_IDS.flutterShop}`,
      `/v1/projects/${PROJECT_IDS.flutterShop}/members`,
      `/v1/projects/${PROJECT_IDS.flutterShop}/members/candidates`,
      `/v1/projects/${PROJECT_IDS.flutterShop}/pipelines`,
      `/v1/projects/${PROJECT_IDS.flutterShop}/repository-workflows`,
      `/v1/builds?project_id=${PROJECT_IDS.flutterShop}`,
      `/v1/projects/${PROJECT_IDS.flutterShop}/artifacts`,
      `/v1/projects/${PROJECT_IDS.flutterShop}/retention`,
      '/v1/builds',
      `/v1/builds/${BUILD_IDS.succeeded1}`,
      `/v1/builds/${BUILD_IDS.succeeded1}/logs`,
      `/v1/builds/${BUILD_IDS.succeeded1}/artifacts`,
      `/v1/pipelines/${PIPELINE_IDS.shopAndroid}`,
      `/v1/pipelines/${PIPELINE_IDS.shopAndroid}/android-signing`,
      `/v1/pipelines/${PIPELINE_IDS.shopIos}/ios-signing`,
      `/v1/pipelines/${PIPELINE_IDS.shopIos}/ios-signing/devices`,
      '/v1/integrations',
      `/v1/integrations/${INTEGRATION_IDS.github}`,
      `/v1/integrations/${INTEGRATION_IDS.github}/repositories`,
      `/v1/integrations/${INTEGRATION_IDS.github}/installations`,
      '/v1/integration-repositories/repo-003/avatar',
      '/v1/runners',
      '/v1/settings/artifact-storage',
      '/v1/settings/preferences',
      '/v1/settings/external-access/preflight',
      '/v1/settings/external-access/network',
      '/v1/settings/external-access/trusted-proxy',
      '/v1/settings/external-access/oidc',
      '/v1/settings/notification-channels',
      `/v1/settings/notification-channels/${NOTIFICATION_CHANNEL_IDS.webhook}`,
      `/v1/settings/notification-channels/${NOTIFICATION_CHANNEL_IDS.webhook}/deliveries`,
      '/v1/settings/retention',
      '/v1/settings/retention/last-cleanup',
      '/v1/audit-logs',
      '/v1/api-tokens',
    ]

    const results = await Promise.all(
      paths.map(async (path) => {
        try {
          const response = await fetch(`${demoOrigin}${path}`, {
            headers: { Authorization: `Bearer ${owner.token}` },
          })
          return { path, status: response.status }
        } catch (error) {
          return { path, error: String(error) }
        }
      }),
    )

    expect(results.filter((result) => result.status !== 200)).toEqual([])
  })
})
