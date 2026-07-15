import { HttpResponse, delay, http } from 'msw'
import { demoBuildEvents, demoBuilds } from '../data/builds'
import { demoBuildLogs } from '../data/build-logs'
import { demoArtifacts } from '../data/artifacts'
import { USER_IDS, ago } from '../seed'
import { getDemoPersonaFromRequest, getDemoProjectRole } from '../personas'

export const buildHandlers = [
  http.get(
    '/v1/projects/:projectId/builds/changelog-preview',
    async ({ request, params }) => {
      await delay(200)
      const persona = getDemoPersonaFromRequest(request)
      if (!getDemoProjectRole(persona, String(params.projectId))) {
        return HttpResponse.json(
          { error: 'Project not found', code: 'not_found' },
          { status: 404 },
        )
      }
      const url = new URL(request.url)
      return HttpResponse.json({
        base_commit: 'b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1',
        target_commit:
          url.searchParams.get('commit_sha') ??
          'a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0',
        markdown:
          '- Faster checkout validation — Alex Morgan\n- Clearer payment retry messaging — Priya Shah\n- Fixed saved delivery addresses — Sam Lee',
      })
    },
  ),

  http.get('/v1/builds', async ({ request }) => {
    await delay(150)
    const url = new URL(request.url)
    const persona = getDemoPersonaFromRequest(request)
    let builds = demoBuilds.filter((build) =>
      getDemoProjectRole(persona, build.project_id),
    )

    const projectId = url.searchParams.get('project_id')
    if (projectId) builds = builds.filter((b) => b.project_id === projectId)

    const pipelineId = url.searchParams.get('pipeline_id')
    if (pipelineId) builds = builds.filter((b) => b.pipeline_id === pipelineId)

    const status = url.searchParams.get('status')
    if (status) builds = builds.filter((b) => b.status === status)

    const branch = url.searchParams.get('branch')
    if (branch) builds = builds.filter((b) => b.branch === branch)

    const limit = Number(url.searchParams.get('limit')) || 50
    const offset = Number(url.searchParams.get('offset')) || 0

    return HttpResponse.json({
      builds: builds.slice(offset, offset + limit),
      total: builds.length,
    })
  }),

  http.get('/v1/builds/:buildId', async ({ params, request }) => {
    await delay(150)
    const persona = getDemoPersonaFromRequest(request)
    const build = demoBuilds.find((b) => b.id === params.buildId)
    if (!build || !getDemoProjectRole(persona, build.project_id)) {
      return HttpResponse.json(
        { error: 'Build not found', code: 'not_found' },
        { status: 404 },
      )
    }
    return HttpResponse.json({
      build,
      events: demoBuildEvents[build.id] ?? [
        {
          id: `evt-auto-${build.id}`,
          build_id: build.id,
          to_status: build.status,
          created_at: build.created_at,
        },
      ],
    })
  }),

  http.post('/v1/projects/:projectId/builds', async ({ params, request }) => {
    await delay(400)
    const body = (await request.json()) as {
      pipeline_id: string
      branch?: string
      commit_sha?: string
      changelog?: string
    }
    return HttpResponse.json({
      build: {
        id: `build-demo-new-${Date.now()}`,
        project_id: params.projectId,
        pipeline_id: body.pipeline_id,
        build_number: 144,
        status: 'queued',
        trigger_type: 'manual',
        trigger_actor: USER_IDS.owner,
        branch: body.branch ?? 'main',
        commit_sha: body.commit_sha,
        changelog: body.changelog,
        config_snapshot: {},
        queued_at: ago(0),
        created_at: ago(0),
        updated_at: ago(0),
      },
    })
  }),

  http.post('/v1/builds/:buildId/cancel', async ({ params }) => {
    await delay(300)
    const build = demoBuilds.find((b) => b.id === params.buildId)
    return HttpResponse.json({
      build: build
        ? {
            ...build,
            status: 'canceled',
            finished_at: ago(0),
            updated_at: ago(0),
          }
        : { id: params.buildId, status: 'canceled' },
    })
  }),

  // Stream token — return 503 to trigger polling fallback in useLogStream
  http.post('/v1/builds/:buildId/stream-token', async () => {
    await delay(100)
    return HttpResponse.json(
      { error: 'Demo mode: live streaming unavailable', code: 'demo_mode' },
      { status: 503 },
    )
  }),

  http.get('/v1/builds/:buildId/logs', async ({ params, request }) => {
    await delay(200)
    const buildId = params.buildId as string
    const persona = getDemoPersonaFromRequest(request)
    const build = demoBuilds.find((candidate) => candidate.id === buildId)
    if (!build || !getDemoProjectRole(persona, build.project_id)) {
      return HttpResponse.json(
        { error: 'Build not found', code: 'not_found' },
        { status: 404 },
      )
    }
    const allLogs = demoBuildLogs[buildId] ?? []

    const url = new URL(request.url)
    const afterSequence = Number(url.searchParams.get('after_sequence') ?? -1)
    const limit = Number(url.searchParams.get('limit')) || 1000

    const filtered = allLogs.filter((l) => l.sequence > afterSequence)
    return HttpResponse.json({
      logs: filtered.slice(0, limit),
      total: allLogs.length,
    })
  }),

  http.get('/v1/builds/:buildId/artifacts', async ({ params, request }) => {
    await delay(150)
    const buildId = params.buildId as string
    const persona = getDemoPersonaFromRequest(request)
    const build = demoBuilds.find((candidate) => candidate.id === buildId)
    if (!build || !getDemoProjectRole(persona, build.project_id)) {
      return HttpResponse.json(
        { error: 'Build not found', code: 'not_found' },
        { status: 404 },
      )
    }
    return HttpResponse.json({
      artifacts: demoArtifacts[buildId] ?? [],
    })
  }),

  http.get('/v1/projects/:projectId/artifacts', async ({ params, request }) => {
    await delay(150)
    const persona = getDemoPersonaFromRequest(request)
    if (!getDemoProjectRole(persona, String(params.projectId))) {
      return HttpResponse.json(
        { error: 'Project not found', code: 'not_found' },
        { status: 404 },
      )
    }
    const projectBuildIds = demoBuilds
      .filter((build) => build.project_id === params.projectId)
      .map((build) => build.id)
    return HttpResponse.json({
      artifacts: projectBuildIds.flatMap(
        (buildId) => demoArtifacts[buildId] ?? [],
      ),
    })
  }),
]
