import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest'
import { setupServer } from 'msw/node'
import { listIntegrationRepos } from '@/lib/api'
import { isDemoMutationAllowed } from '@/lib/demo-mode'
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
import {
  EXTRA_BUILD_IDS,
  EXTRA_PIPELINE_IDS,
  EXTRA_PROJECT_IDS,
  PAGINATED_PIPELINE_PROJECT_ID,
  createDemoState,
  demoState,
  resetDemoState,
} from './state'

const server = setupServer(...allHandlers)
const demoOrigin = window.location.origin

function persona(role: (typeof DEMO_PERSONAS)[number]['role']) {
  return DEMO_PERSONAS.find((candidate) => candidate.role === role)!
}

function headers(role: (typeof DEMO_PERSONAS)[number]['role']) {
  return { Authorization: `Bearer ${persona(role).token}` }
}

beforeAll(() => server.listen({ onUnhandledRequest: 'error' }))
afterEach(() => {
  resetDemoState()
  server.resetHandlers()
})
afterAll(() => server.close())

describe('canonical demo state', () => {
  it('has valid entity relationships and useful collection scale', () => {
    const state = createDemoState()
    const projectIds = new Set(state.projects.map((project) => project.id))
    const pipelineIds = new Set(state.pipelines.map((pipeline) => pipeline.id))
    const buildIds = new Set(state.builds.map((build) => build.id))
    const runnerIds = new Set(state.runners.map((runner) => runner.id))
    const userIds = new Set(state.users.map((user) => user.id))
    const repositoryIds = new Set(
      Object.values(state.repositories)
        .flatMap((repositories) => repositories ?? [])
        .map((repository) => repository.id),
    )
    const channelIds = new Set(
      state.notificationChannels.map((channel) => channel.id),
    )

    expect(state.projects).toHaveLength(24)
    expect(repositoryIds.size).toBe(56)
    expect(
      state.projects.every(
        (project) =>
          !project.repository_id || repositoryIds.has(project.repository_id),
      ),
    ).toBe(true)
    expect(
      state.pipelines.every((pipeline) => projectIds.has(pipeline.project_id)),
    ).toBe(true)
    expect(
      state.pipelines.filter(
        (pipeline) => pipeline.project_id === PAGINATED_PIPELINE_PROJECT_ID,
      ),
    ).toHaveLength(25)
    expect(
      state.builds.every(
        (build) =>
          projectIds.has(build.project_id) &&
          pipelineIds.has(build.pipeline_id) &&
          (!build.runner_id || runnerIds.has(build.runner_id)),
      ),
    ).toBe(true)
    expect(
      Object.values(state.artifacts)
        .flatMap((artifacts) => artifacts ?? [])
        .every((artifact) => buildIds.has(artifact.build_id)),
    ).toBe(true)
    expect(
      state.notificationDeliveries.every(
        (delivery) =>
          channelIds.has(delivery.channel_id) &&
          !!delivery.build_id &&
          buildIds.has(delivery.build_id),
      ),
    ).toBe(true)
    expect(
      Object.entries(state.projectRoles).every(
        ([projectId, roles]) =>
          projectIds.has(projectId) &&
          !!roles &&
          Object.keys(roles).every((userId) => userIds.has(userId)),
      ),
    ).toBe(true)
  })

  it('covers every status, trigger, runner, artifact, delivery, and user state', () => {
    expect(new Set(demoState.builds.map((build) => build.status))).toEqual(
      new Set([
        'queued',
        'scheduled',
        'assigned',
        'running',
        'succeeded',
        'failed',
        'canceled',
        'timed_out',
        'expired',
      ]),
    )
    expect(
      new Set(demoState.builds.map((build) => build.trigger_type)),
    ).toEqual(new Set(['manual', 'api', 'webhook', 'schedule']))
    expect(new Set(demoState.runners.map((runner) => runner.status))).toEqual(
      new Set(['online', 'offline', 'busy', 'draining']),
    )
    expect(
      new Set(
        Object.values(demoState.artifacts)
          .flatMap((artifacts) => artifacts ?? [])
          .map((artifact) => artifact.artifact_type),
      ),
    ).toEqual(new Set(['apk', 'ipa', 'app', 'generic']))
    expect(
      new Set(
        demoState.notificationDeliveries.map((delivery) => delivery.status),
      ),
    ).toEqual(new Set(['pending', 'delivered', 'failed']))
    expect(new Set(demoState.users.map((user) => user.status))).toEqual(
      new Set(['active', 'invited', 'disabled']),
    )
  })

  it('applies operating, blocked, degraded, empty, and setup overlays', () => {
    expect(
      resetDemoState('operating').preferences.direct_macos_runner_paused,
    ).toBe(false)

    const blocked = resetDemoState('blocked')
    expect(blocked.preferences.direct_macos_runner_paused).toBe(true)
    expect(
      blocked.builds.find((build) => build.id === EXTRA_BUILD_IDS.policyBlocked)
        ?.runner_policy_block_reason,
    ).toBe('instance_paused')

    const degraded = resetDemoState('degraded')
    expect(
      degraded.integrations.some(
        (integration) => integration.status === 'error',
      ),
    ).toBe(true)
    expect(
      degraded.builds.find(
        (build) => build.id === EXTRA_BUILD_IDS.policyBlocked,
      )?.runner_policy_block_reason,
    ).toBe('repository_unavailable')

    const empty = resetDemoState('empty')
    expect(empty.projects).toEqual([])
    expect(empty.builds).toEqual([])

    const setup = resetDemoState('setup')
    expect(setup.setupStatus).toMatchObject({
      state: 'bootstrap_pending',
      setup_mode: true,
      is_configured: false,
    })
  })
})

describe('demo authentication and RBAC', () => {
  it('accepts the documented accounts and rejects invalid credentials', () => {
    for (const account of DEMO_PERSONAS) {
      expect(authenticateDemoUser(account.email, DEMO_PASSWORD)?.role).toBe(
        account.role,
      )
    }
    expect(authenticateDemoUser(DEMO_PERSONAS[0].email, 'wrong')).toBeNull()
  })

  it('models maintainer, developer, viewer, and unassigned project access', () => {
    const developer = persona('developer')
    expect(getDemoProjectRole(developer, PROJECT_IDS.flutterShop)).toBe(
      'maintainer',
    )
    expect(
      getDemoProjectRole(developer, EXTRA_PROJECT_IDS.developerTools),
    ).toBe('developer')
    expect(getDemoProjectRole(developer, PROJECT_IDS.nativePayments)).toBe(
      'viewer',
    )
    expect(getDemoProjectRole(developer, PROJECT_IDS.internalAdmin)).toBeNull()
  })

  it('returns 401 for missing and invalid protected credentials', async () => {
    const [missing, invalid] = await Promise.all([
      fetch(`${demoOrigin}/v1/users/me`),
      fetch(`${demoOrigin}/v1/projects`, {
        headers: { Authorization: 'Bearer not-a-demo-token' },
      }),
    ])
    expect(missing.status).toBe(401)
    expect(invalid.status).toBe(401)
    await expect(missing.json()).resolves.toMatchObject({
      code: 'unauthorized',
    })
    await expect(invalid.json()).resolves.toMatchObject({
      code: 'unauthorized',
    })
  })

  it.each([
    ['owner', 24],
    ['admin', 24],
    ['developer', 3],
    ['qa_viewer', 2],
  ] as const)('%s sees the truthful project scope', async (role, count) => {
    const response = await fetch(`${demoOrigin}/v1/projects`, {
      headers: headers(role),
    })
    const body = (await response.json()) as {
      projects: Array<{ current_user_role?: string }>
      total: number
    }
    expect(response.status).toBe(200)
    expect(body.total).toBe(count)
    expect(body.projects).toHaveLength(count)
    expect(body.projects.every((project) => project.current_user_role)).toBe(
      true,
    )
  })

  it('returns JSON 404 for an unassigned project', async () => {
    const response = await fetch(
      `${demoOrigin}/v1/projects/${PROJECT_IDS.internalAdmin}`,
      { headers: headers('developer') },
    )

    expect(response.status).toBe(404)
    expect(response.headers.get('content-type')).toContain('application/json')
    await expect(response.json()).resolves.toMatchObject({ code: 'not_found' })
  })

  it('keeps instance settings and user inventory admin-only', async () => {
    const [users, preferences] = await Promise.all([
      fetch(`${demoOrigin}/v1/users`, { headers: headers('developer') }),
      fetch(`${demoOrigin}/v1/settings/preferences`, {
        headers: headers('developer'),
      }),
    ])

    expect(users.status).toBe(403)
    expect(preferences.status).toBe(403)
    await expect(users.json()).resolves.toMatchObject({ code: 'forbidden' })
    await expect(preferences.json()).resolves.toMatchObject({
      code: 'forbidden',
    })
  })

  it('scopes eligible member candidates to project maintainers', async () => {
    const [maintainer, viewer] = await Promise.all([
      fetch(
        `${demoOrigin}/v1/projects/${PROJECT_IDS.flutterShop}/members/candidates`,
        { headers: headers('developer') },
      ),
      fetch(
        `${demoOrigin}/v1/projects/${PROJECT_IDS.nativePayments}/members/candidates`,
        { headers: headers('developer') },
      ),
    ])

    expect(maintainer.status).toBe(200)
    await expect(maintainer.json()).resolves.toEqual({
      candidates: [
        expect.objectContaining({ id: USER_IDS.invited, role: 'developer' }),
      ],
    })
    expect(viewer.status).toBe(403)
  })

  it('enforces instance and project permissions', async () => {
    const developerHeaders = {
      ...headers('developer'),
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
        headers: developerHeaders,
        body: JSON.stringify({
          email: 'demo+new@oore.build',
          role: 'developer',
        }),
      }),
      fetch(`${demoOrigin}/v1/projects`, {
        method: 'POST',
        headers: developerHeaders,
        body: JSON.stringify({ name: 'Developer project' }),
      }),
      fetch(`${demoOrigin}/v1/projects/${PROJECT_IDS.nativePayments}`, {
        method: 'PATCH',
        headers: developerHeaders,
        body: JSON.stringify({ name: 'Should not change' }),
      }),
      fetch(`${demoOrigin}/v1/projects/${PROJECT_IDS.flutterShop}`, {
        method: 'PATCH',
        headers: developerHeaders,
        body: JSON.stringify({ repository_id: 'repo-other' }),
      }),
      fetch(
        `${demoOrigin}/v1/projects/${EXTRA_PROJECT_IDS.developerTools}/builds`,
        {
          method: 'POST',
          headers: developerHeaders,
          body: JSON.stringify({
            pipeline_id: EXTRA_PIPELINE_IDS.developerTools,
          }),
        },
      ),
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
        headers: developerHeaders,
        body: JSON.stringify({ pipeline_id: PIPELINE_IDS.paymentsAll }),
      },
    )
    expect(runViewerProject.status).toBe(403)

    const [deleteViewerProject, deleteViewerPipeline, manageViewerMembers] =
      await Promise.all([
        fetch(`${demoOrigin}/v1/projects/${PROJECT_IDS.nativePayments}`, {
          method: 'DELETE',
          headers: developerHeaders,
        }),
        fetch(`${demoOrigin}/v1/pipelines/${PIPELINE_IDS.paymentsAll}`, {
          method: 'DELETE',
          headers: developerHeaders,
        }),
        fetch(
          `${demoOrigin}/v1/projects/${PROJECT_IDS.nativePayments}/members/${USER_IDS.qaViewer}`,
          {
            method: 'PATCH',
            headers: developerHeaders,
            body: JSON.stringify({ role: 'viewer' }),
          },
        ),
      ])
    expect(deleteViewerProject.status).toBe(403)
    expect(deleteViewerPipeline.status).toBe(403)
    expect(manageViewerMembers.status).toBe(403)

    const manageMaintainerMembers = await fetch(
      `${demoOrigin}/v1/projects/${PROJECT_IDS.flutterShop}/members/${USER_IDS.qaViewer}`,
      {
        method: 'PATCH',
        headers: developerHeaders,
        body: JSON.stringify({ role: 'viewer' }),
      },
    )
    expect(manageMaintainerMembers.status).toBe(200)

    const deleteMaintainerPipeline = await fetch(
      `${demoOrigin}/v1/pipelines/${PIPELINE_IDS.shopAndroid}`,
      { method: 'DELETE', headers: developerHeaders },
    )
    expect(deleteMaintainerPipeline.status).toBe(204)

    const deleteMaintainerProject = await fetch(
      `${demoOrigin}/v1/projects/${PROJECT_IDS.flutterShop}`,
      { method: 'DELETE', headers: developerHeaders },
    )
    expect(deleteMaintainerProject.status).toBe(204)
  })

  it('keeps hosted demo writes blocked and local demo writes interactive', async () => {
    expect(
      isDemoMutationAllowed('POST', '/v1/projects', 'demo.oore.build'),
    ).toBe(false)
    expect(isDemoMutationAllowed('POST', '/v1/projects', 'localhost')).toBe(
      true,
    )
    expect(
      isDemoMutationAllowed(
        'POST',
        '/v1/artifacts/art-001/download-link',
        'demo.oore.build',
      ),
    ).toBe(true)
    expect(
      isDemoMutationAllowed(
        'POST',
        '/v1/telemetry/web-performance',
        'demo.oore.build',
      ),
    ).toBe(true)
    expect(
      isDemoMutationAllowed(
        'DELETE',
        '/v1/telemetry/web-performance',
        'demo.oore.build',
      ),
    ).toBe(false)

    const projectCount = demoState.projects.length
    const hostedWrite = await fetch('https://demo.oore.build/v1/projects', {
      method: 'POST',
      headers: {
        ...headers('owner'),
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ name: 'Must stay read-only' }),
    })
    expect(hostedWrite.status).toBe(403)
    await expect(hostedWrite.json()).resolves.toMatchObject({
      code: 'demo_read_only',
    })
    expect(demoState.projects).toHaveLength(projectCount)
  })
})

describe('interactive demo API', () => {
  it('matches backend pipeline search, sorting, and pagination', async () => {
    const page = await fetch(
      `${demoOrigin}/v1/projects/${PAGINATED_PIPELINE_PROJECT_ID}/pipelines?sort=name&direction=asc&limit=20&offset=20`,
      { headers: headers('owner') },
    )
    const pageBody = (await page.json()) as {
      pipelines: Array<{ name: string }>
      total: number
    }

    expect(page.status).toBe(200)
    expect(pageBody.total).toBe(25)
    expect(pageBody.pipelines.map((pipeline) => pipeline.name)).toEqual([
      'Release candidate 05',
      'Release candidate 06',
      'Release candidate 07',
      'Release candidate 08',
      'Release candidate 09',
    ])

    const search = await fetch(
      `${demoOrigin}/v1/projects/${PAGINATED_PIPELINE_PROJECT_ID}/pipelines?search=%20RELEASE%20candidate%20&sort=name&direction=desc&limit=4&offset=0`,
      { headers: headers('owner') },
    )
    const searchBody = (await search.json()) as {
      pipelines: Array<{ name: string }>
      total: number
    }

    expect(search.status).toBe(200)
    expect(searchBody.total).toBe(9)
    expect(searchBody.pipelines.map((pipeline) => pipeline.name)).toEqual([
      'Release candidate 09',
      'Release candidate 08',
      'Release candidate 07',
      'Release candidate 06',
    ])
  })

  it('accepts authenticated web telemetry in local and hosted demos', async () => {
    const request = (
      origin: string,
      authorization: Record<string, string> = headers('owner'),
    ) =>
      fetch(`${origin}/v1/telemetry/web-performance`, {
        method: 'POST',
        headers: {
          ...authorization,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ metric: 'LCP', value: 420 }),
      })

    const [local, hosted, missingAuth] = await Promise.all([
      request(demoOrigin),
      request('https://demo.oore.build'),
      request(demoOrigin, {}),
    ])

    expect(local.status).toBe(204)
    expect(hosted.status).toBe(204)
    expect(missingAuth.status).toBe(401)
  })

  it('serves degraded and setup scenario behavior', async () => {
    resetDemoState('degraded')
    const [repositories, update] = await Promise.all([
      fetch(
        `${demoOrigin}/v1/integrations/${INTEGRATION_IDS.github}/repositories`,
        { headers: headers('owner') },
      ),
      fetch(`${demoOrigin}/v1/system/update`, { headers: headers('owner') }),
    ])
    expect(repositories.status).toBe(503)
    await expect(update.json()).resolves.toMatchObject({ phase: 'failed' })

    resetDemoState('setup')
    const setup = await fetch(`${demoOrigin}/v1/public/setup-status`)
    await expect(setup.json()).resolves.toMatchObject({
      state: 'bootstrap_pending',
      is_configured: false,
    })
  })

  it('persists create, update, and delete mutations in the session graph', async () => {
    const ownerHeaders = {
      ...headers('owner'),
      'Content-Type': 'application/json',
    }
    const create = await fetch(`${demoOrigin}/v1/projects`, {
      method: 'POST',
      headers: ownerHeaders,
      body: JSON.stringify({ name: 'Session Project' }),
    })
    const created = (await create.json()) as { project: { id: string } }

    const update = await fetch(
      `${demoOrigin}/v1/projects/${created.project.id}`,
      {
        method: 'PATCH',
        headers: ownerHeaders,
        body: JSON.stringify({ name: 'Updated Session Project' }),
      },
    )
    await expect(update.json()).resolves.toMatchObject({
      project: { name: 'Updated Session Project' },
    })

    const list = await fetch(
      `${demoOrigin}/v1/projects?search=updated%20session`,
      { headers: headers('owner') },
    )
    await expect(list.json()).resolves.toMatchObject({ total: 1 })

    const remove = await fetch(
      `${demoOrigin}/v1/projects/${created.project.id}`,
      { method: 'DELETE', headers: ownerHeaders },
    )
    expect(remove.status).toBe(204)
    const missing = await fetch(
      `${demoOrigin}/v1/projects/${created.project.id}`,
      { headers: headers('owner') },
    )
    expect(missing.status).toBe(404)
  })

  it('paginates repository responses and terminates aggregate loading', async () => {
    const owner = persona('owner')
    const page = async (offset: number) => {
      const response = await fetch(
        `${demoOrigin}/v1/integrations/${INTEGRATION_IDS.github}/repositories?limit=20&offset=${offset}`,
        { headers: headers('owner') },
      )
      return (await response.json()) as {
        repositories: Array<{ id: string }>
      }
    }

    const [first, second, third, terminal] = await Promise.all([
      page(0),
      page(20),
      page(40),
      page(60),
    ])
    expect(first.repositories).toHaveLength(20)
    expect(second.repositories).toHaveLength(20)
    expect(third.repositories).toHaveLength(15)
    expect(terminal.repositories).toHaveLength(0)
    expect(first.repositories[0]?.id).not.toBe(second.repositories[0]?.id)

    const aggregate = await listIntegrationRepos(
      demoOrigin,
      owner.token,
      INTEGRATION_IDS.github,
    )
    expect(aggregate.repositories).toHaveLength(55)
  })

  it('lists API tokens with backend role scope', async () => {
    const [ownerResponse, developerResponse] = await Promise.all([
      fetch(`${demoOrigin}/v1/api-tokens`, { headers: headers('owner') }),
      fetch(`${demoOrigin}/v1/api-tokens`, { headers: headers('developer') }),
    ])
    const ownerBody = (await ownerResponse.json()) as { tokens: Array<unknown> }
    const developerBody = (await developerResponse.json()) as {
      tokens: Array<{ created_by: string }>
    }
    expect(ownerBody.tokens).toHaveLength(5)
    expect(developerBody.tokens).toHaveLength(2)
    expect(
      developerBody.tokens.every(
        (token) => token.created_by === USER_IDS.developer,
      ),
    ).toBe(true)
  })

  it('serves every authenticated read surface without fallthrough', async () => {
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
      `/v1/projects/${EXTRA_PROJECT_IDS.workflowOnly}/repository-workflows`,
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
      paths.map(async (path) => ({
        path,
        status: (
          await fetch(`${demoOrigin}${path}`, { headers: headers('owner') })
        ).status,
      })),
    )
    expect(results.filter((result) => result.status !== 200)).toEqual([])
  })
})
