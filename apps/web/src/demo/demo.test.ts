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
      'developer',
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
