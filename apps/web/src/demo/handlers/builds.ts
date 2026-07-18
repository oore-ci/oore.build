import { HttpResponse, delay, http } from 'msw'
import { ago } from '../seed'
import { getDemoPersonaFromRequest, getDemoProjectRole } from '../personas'
import { requireDemoProjectPermission } from '../authorization'
import { demoState } from '../state'

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
    let builds = demoState.builds.filter((build) =>
      getDemoProjectRole(persona, build.project_id),
    )

    const projectId = url.searchParams.get('project_id')
    if (projectId) builds = builds.filter((b) => b.project_id === projectId)

    const pipelineId = url.searchParams.get('pipeline_id')
    if (pipelineId) builds = builds.filter((b) => b.pipeline_id === pipelineId)

    if (url.searchParams.has('status')) {
      const statuses = [
        ...new Set(
          (url.searchParams.get('status') ?? '')
            .split(',')
            .map((status) => status.trim())
            .filter(Boolean),
        ),
      ]
      if (statuses.length > 9) {
        return HttpResponse.json(
          {
            error: 'status accepts at most 9 comma-separated values',
            code: 'invalid_input',
          },
          { status: 400 },
        )
      }
      builds = builds.filter((build) => statuses.includes(build.status))
    }

    const branch = url.searchParams.get('branch')
    if (branch) builds = builds.filter((b) => b.branch === branch)

    const sort = url.searchParams.get('sort')
    const direction = url.searchParams.get('direction') === 'asc' ? 1 : -1
    builds.sort((left, right) => {
      const value = (build: (typeof demoState.builds)[number]) => {
        if (sort === 'status') return build.status.toLowerCase()
        if (sort === 'branch') return build.branch?.toLowerCase() ?? ''
        if (sort === 'project_name') {
          return (
            demoState.projects
              .find((project) => project.id === build.project_id)
              ?.name.toLowerCase() ?? ''
          )
        }
        if (sort === 'pipeline_name') {
          return (
            demoState.pipelines
              .find((pipeline) => pipeline.id === build.pipeline_id)
              ?.name.toLowerCase() ?? ''
          )
        }
        return build.created_at
      }
      const leftValue = value(left)
      const rightValue = value(right)
      const compared =
        typeof leftValue === 'string'
          ? leftValue.localeCompare(String(rightValue))
          : leftValue - Number(rightValue)
      return (compared || left.id.localeCompare(right.id)) * direction
    })

    const limit = Math.min(Number(url.searchParams.get('limit')) || 50, 200)
    const offset = Number(url.searchParams.get('offset')) || 0

    return HttpResponse.json({
      builds: builds.slice(offset, offset + limit),
      total: builds.length,
    })
  }),

  http.get('/v1/builds/:buildId', async ({ params, request }) => {
    await delay(150)
    const persona = getDemoPersonaFromRequest(request)
    const build = demoState.builds.find((b) => b.id === params.buildId)
    if (!build || !getDemoProjectRole(persona, build.project_id)) {
      return HttpResponse.json(
        { error: 'Build not found', code: 'not_found' },
        { status: 404 },
      )
    }
    return HttpResponse.json({
      build,
      events: demoState.buildEvents[build.id] ?? [
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
    const forbidden = requireDemoProjectPermission(
      request,
      String(params.projectId),
      'builds:write',
    )
    if (forbidden) return forbidden
    const persona = getDemoPersonaFromRequest(request)
    const body = (await request.json()) as {
      pipeline_id: string
      branch?: string
      commit_sha?: string
      changelog?: string
    }
    const projectId = String(params.projectId)
    const pipeline = demoState.pipelines.find(
      (candidate) =>
        candidate.id === body.pipeline_id && candidate.project_id === projectId,
    )
    if (!pipeline) {
      return HttpResponse.json(
        { error: 'Pipeline not found', code: 'not_found' },
        { status: 404 },
      )
    }
    const build = {
      id: `build-demo-new-${crypto.randomUUID().slice(0, 8)}`,
      project_id: projectId,
      pipeline_id: body.pipeline_id,
      build_number:
        Math.max(
          0,
          ...demoState.builds
            .filter((candidate) => candidate.project_id === projectId)
            .map((candidate) => candidate.build_number),
        ) + 1,
      status: 'queued' as const,
      trigger_type: 'manual' as const,
      trigger_actor: persona.userId,
      branch: body.branch ?? 'main',
      commit_sha: body.commit_sha,
      changelog: body.changelog,
      config_snapshot: {},
      queued_at: ago(0),
      created_at: ago(0),
      updated_at: ago(0),
    }
    demoState.builds.unshift(build)
    demoState.buildEvents[build.id] = [
      {
        id: `evt-${build.id}-queued`,
        build_id: build.id,
        to_status: 'queued',
        actor: persona.userId,
        created_at: build.created_at,
      },
    ]
    return HttpResponse.json({ build })
  }),

  http.post('/v1/builds/:buildId/cancel', async ({ params, request }) => {
    await delay(300)
    const build = demoState.builds.find((b) => b.id === params.buildId)
    if (!build) {
      return HttpResponse.json(
        { error: 'Build not found', code: 'not_found' },
        { status: 404 },
      )
    }
    const forbidden = requireDemoProjectPermission(
      request,
      build.project_id,
      'builds:cancel',
    )
    if (forbidden) return forbidden
    const persona = getDemoPersonaFromRequest(request)
    const fromStatus = build.status
    build.status = 'canceled'
    build.finished_at = ago(0)
    build.updated_at = ago(0)
    const events = (demoState.buildEvents[build.id] ??= [])
    events.push({
      id: `evt-${build.id}-canceled-${Date.now()}`,
      build_id: build.id,
      from_status: fromStatus,
      to_status: 'canceled',
      actor: persona.userId,
      created_at: build.updated_at,
    })
    return HttpResponse.json({ build })
  }),

  http.post('/v1/builds/:buildId/rerun', async ({ params, request }) => {
    await delay(300)
    const build = demoState.builds.find(
      (candidate) => candidate.id === params.buildId,
    )
    if (!build) {
      return HttpResponse.json(
        { error: 'Build not found', code: 'not_found' },
        { status: 404 },
      )
    }
    const forbidden = requireDemoProjectPermission(
      request,
      build.project_id,
      'builds:write',
    )
    if (forbidden) return forbidden
    const rerun = {
      ...build,
      id: `build-demo-rerun-${crypto.randomUUID().slice(0, 8)}`,
      build_number:
        Math.max(
          build.build_number,
          ...demoState.builds
            .filter((candidate) => candidate.project_id === build.project_id)
            .map((candidate) => candidate.build_number),
        ) + 1,
      status: 'queued' as const,
      source_build_id: build.id,
      runner_id: undefined,
      started_at: undefined,
      finished_at: undefined,
      exit_code: undefined,
      queued_at: ago(0),
      created_at: ago(0),
      updated_at: ago(0),
    }
    demoState.builds.unshift(rerun)
    demoState.buildEvents[rerun.id] = [
      {
        id: `evt-${rerun.id}-queued`,
        build_id: rerun.id,
        to_status: 'queued',
        created_at: rerun.created_at,
      },
    ]
    return HttpResponse.json({ build: rerun })
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
    const build = demoState.builds.find((candidate) => candidate.id === buildId)
    if (!build || !getDemoProjectRole(persona, build.project_id)) {
      return HttpResponse.json(
        { error: 'Build not found', code: 'not_found' },
        { status: 404 },
      )
    }
    const allLogs = demoState.buildLogs[buildId] ?? []

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
    const build = demoState.builds.find((candidate) => candidate.id === buildId)
    if (!build || !getDemoProjectRole(persona, build.project_id)) {
      return HttpResponse.json(
        { error: 'Build not found', code: 'not_found' },
        { status: 404 },
      )
    }
    return HttpResponse.json({
      artifacts: demoState.artifacts[buildId] ?? [],
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
    const projectBuildIds = demoState.builds
      .filter((build) => build.project_id === params.projectId)
      .map((build) => build.id)
    const url = new URL(request.url)
    const limit = Math.min(
      Number(url.searchParams.get('limit')) || Number.POSITIVE_INFINITY,
      200,
    )
    const artifacts = projectBuildIds
      .flatMap((buildId) => demoState.artifacts[buildId] ?? [])
      .sort(
        (left, right) =>
          right.created_at - left.created_at || right.id.localeCompare(left.id),
      )
      .slice(0, limit)
    return HttpResponse.json({ artifacts })
  }),

  http.post('/v1/artifacts/query', async ({ request }) => {
    await delay(150)
    const persona = getDemoPersonaFromRequest(request)
    const body = (await request.json()) as { build_ids: Array<string> }
    const visibleBuildIds = new Set(
      demoState.builds
        .filter(
          (build) =>
            body.build_ids.includes(build.id) &&
            getDemoProjectRole(persona, build.project_id),
        )
        .map((build) => build.id),
    )
    return HttpResponse.json({
      artifacts: [...visibleBuildIds].flatMap(
        (buildId) => demoState.artifacts[buildId] ?? [],
      ),
    })
  }),
]
