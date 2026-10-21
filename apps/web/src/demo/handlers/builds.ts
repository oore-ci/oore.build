import { demoApi } from './api'
import { HttpResponse, delay } from 'msw'
import * as z from 'zod'
import { ago } from '../seed'
import { getDemoPersonaFromRequest, getDemoProjectRole } from '../personas'
import { requireDemoProjectPermission } from '../authorization'
import { demoState } from '../state'
import { demoBuildInsights } from '../data/build-insights'
import type { CreateBuildRequest } from '@oore/client/models'

const approvalDecisionSchema = z.object({ reason: z.string().nullish() })

const createBuildRequestSchema = z.toZod<CreateBuildRequest>()(
  z.object({
    pipeline_id: z.string(),
    branch: z.string().nullish(),
    commit_sha: z.string().nullish(),
    changelog: z.string().nullish(),
    platforms: z.array(z.enum(['android', 'ios', 'macos'])).nullish(),
    trigger_ref: z.string().nullish(),
  }),
)
const artifactQuerySchema = z.object({ build_ids: z.array(z.string()) })

function withBuildContext(build: (typeof demoState.builds)[number]) {
  const project = demoState.projects.find(
    (candidate) => candidate.id === build.project_id,
  )
  const pipeline = demoState.pipelines.find(
    (candidate) => candidate.id === build.pipeline_id,
  )
  const integrationId = Object.entries(demoState.repositories).find(
    ([, repositories]) =>
      repositories?.some(
        (repository) => repository.id === project?.repository_id,
      ),
  )?.[0]
  const integration = demoState.integrations.find(
    (candidate) => candidate.id === integrationId,
  )

  return {
    ...build,
    config_snapshot:
      build.config_snapshot.ui_execution_config || !pipeline
        ? build.config_snapshot
        : {
            ...build.config_snapshot,
            ui_execution_config: pipeline.execution_config,
          },
    context: {
      project_name: project?.name,
      repository_id: project?.repository_id,
      project_avatar_url: project?.repository_avatar_url,
      repository_full_name: project?.repository_full_name,
      repository_provider: integration?.provider,
      repository_host_url: integration?.host_url,
      pipeline_name: pipeline?.name,
      runner_name: demoState.runners.find(
        (runner) => runner.id === build.runner_id,
      )?.name,
    },
  }
}

const RETRYABLE_STEPS = ['publish-play', 'publish-app-store']

/** Uploads the demo can retry: the artifacts were stored and the upload did not succeed. */
function demoRetryableSteps(build: (typeof demoState.builds)[number]) {
  const steps = build.step_results ?? []
  const terminal = ['succeeded', 'failed', 'canceled', 'timed_out'].includes(
    build.status,
  )
  const stored = steps.some(
    (step) => step.name === 'artifacts' && step.status === 'succeeded',
  )
  if (!terminal || !stored) return []
  return RETRYABLE_STEPS.filter((name) =>
    steps.some((step) => step.name === name && step.status !== 'succeeded'),
  )
}

export const buildHandlers = [
  demoApi.previewBuildChangelog(async ({ request, params }) => {
    await delay(200)
    const persona = getDemoPersonaFromRequest(request)
    if (!getDemoProjectRole(persona, String(params.project_id))) {
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
      markdown: [
        '- Faster checkout validation — Alex Morgan',
        '- Clearer payment retry messaging — Priya Shah',
        '- Fixed saved delivery addresses — Sam Lee',
        '- Added retry guidance for failed jobs — Mina Patel',
        '- Reduced Android checkout time — Chen Wei',
        '- Updated iOS signing diagnostics — Jordan Kim',
        '- Preserved build filters after refresh — Alex Morgan',
        '- Fixed long artifact names — Priya Shah',
        '- Added runner capacity details — Sam Lee',
        '- Improved source connection errors — Mina Patel',
        '- Clarified manual build permissions — Chen Wei',
        '- Fixed pipeline branch selection — Jordan Kim',
        '- Added deployment status details — Alex Morgan',
        '- Reduced duplicate webhook work — Priya Shah',
        '- Improved failed-step log links — Sam Lee',
        '- Fixed project role updates — Mina Patel',
        '- Added clearer empty states — Chen Wei',
        '- Improved keyboard navigation — Jordan Kim',
      ].join('\n'),
    })
  }),

  demoApi.listBuilds(async ({ request }) => {
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
      const leftString = z.string().safeParse(leftValue)
      const compared = leftString.success
        ? leftString.data.localeCompare(String(rightValue))
        : Number(leftValue) - Number(rightValue)
      return (compared || left.id.localeCompare(right.id)) * direction
    })

    const limit = Math.min(Number(url.searchParams.get('limit')) || 50, 200)
    const offset = Number(url.searchParams.get('offset')) || 0

    return HttpResponse.json({
      builds: builds.slice(offset, offset + limit).map(withBuildContext),
      total: builds.length,
    })
  }),

  demoApi.getBuildInsights(async ({ request }) => {
    await delay(200)
    const url = new URL(request.url)
    const persona = getDemoPersonaFromRequest(request)
    const projectId = url.searchParams.get('project_id')
    const builds = demoState.builds.filter(
      (build) =>
        getDemoProjectRole(persona, build.project_id) &&
        (!projectId || build.project_id === projectId),
    )
    return HttpResponse.json(
      demoBuildInsights({
        builds,
        days: Number(url.searchParams.get('days') ?? 7),
        now: ago(0),
        offsetMinutes: Number(url.searchParams.get('utc_offset_minutes') ?? 0),
        pipelines: demoState.pipelines,
        projects: demoState.projects,
      }),
    )
  }),

  demoApi.getBuild(async ({ params, request }) => {
    await delay(150)
    const persona = getDemoPersonaFromRequest(request)
    const build = demoState.builds.find((b) => b.id === params.build_id)
    if (!build || !getDemoProjectRole(persona, build.project_id)) {
      return HttpResponse.json(
        { error: 'Build not found', code: 'not_found' },
        { status: 404 },
      )
    }
    return HttpResponse.json({
      build: withBuildContext(build),
      retryable_steps: demoRetryableSteps(build),
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

  demoApi.createBuild(async ({ params, request }) => {
    await delay(400)
    const forbidden = requireDemoProjectPermission(
      request,
      String(params.project_id),
      'builds:write',
    )
    if (forbidden) return forbidden
    const persona = getDemoPersonaFromRequest(request)
    const body = createBuildRequestSchema.parse(await request.json())
    const projectId = String(params.project_id)
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
    const build: (typeof demoState.builds)[number] = {
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
      status: 'queued',
      trigger_type: 'manual',
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
    return HttpResponse.json(
      { build: withBuildContext(build) },
      { status: 201 },
    )
  }),

  demoApi.cancelBuild(async ({ params, request }) => {
    await delay(300)
    const build = demoState.builds.find((b) => b.id === params.build_id)
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
    return HttpResponse.json({ build: withBuildContext(build) })
  }),

  ...(['approve', 'reject'] as const).map((decision) =>
    demoApi[decision === 'approve' ? 'approveBuild' : 'rejectBuild'](
      async ({ params, request }) => {
        await delay(300)
        const build = demoState.builds.find((b) => b.id === params.build_id)
        if (!build) {
          return HttpResponse.json(
            { error: 'Build not found', code: 'not_found' },
            { status: 404 },
          )
        }
        const forbidden = requireDemoProjectPermission(
          request,
          build.project_id,
          'builds:approve',
        )
        if (forbidden) return forbidden
        if (build.status !== 'queued' || build.approval?.state !== 'pending') {
          return HttpResponse.json(
            {
              error: 'This build is not waiting for approval',
              code: 'not_awaiting_approval',
            },
            { status: 409 },
          )
        }
        const persona = getDemoPersonaFromRequest(request)
        const body = approvalDecisionSchema.parse(
          await request.json().catch(() => ({})),
        )
        const reason = body.reason?.trim() || undefined
        const approved = decision === 'approve'
        build.approval = {
          ...build.approval,
          state: approved ? 'approved' : 'rejected',
          decided_by: persona.email,
          decided_at: ago(0),
          reason,
        }
        build.updated_at = ago(0)
        if (!approved) {
          build.status = 'canceled'
          build.finished_at = ago(0)
        }
        const verb = approved ? 'approved' : 'rejected'
        ;(demoState.buildEvents[build.id] ??= []).push({
          id: `evt-${build.id}-${verb}-${Date.now()}`,
          build_id: build.id,
          from_status: 'queued',
          to_status: build.status,
          actor: persona.email,
          reason: `${verb} by ${persona.email}${reason ? `: ${reason}` : ''}`,
          created_at: build.updated_at,
        })
        return HttpResponse.json({ build: withBuildContext(build) })
      },
    ),
  ),

  demoApi.rerunBuild(async ({ params, request }) => {
    await delay(300)
    const build = demoState.builds.find(
      (candidate) => candidate.id === params.build_id,
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
    const rerun: (typeof demoState.builds)[number] = {
      ...build,
      id: `build-demo-rerun-${crypto.randomUUID().slice(0, 8)}`,
      build_number:
        Math.max(
          build.build_number,
          ...demoState.builds
            .filter((candidate) => candidate.project_id === build.project_id)
            .map((candidate) => candidate.build_number),
        ) + 1,
      status: 'queued',
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
    return HttpResponse.json(
      { build: withBuildContext(rerun) },
      { status: 201 },
    )
  }),

  demoApi.retryBuildStep(async ({ params, request }) => {
    await delay(300)
    const build = demoState.builds.find(
      (candidate) => candidate.id === params.build_id,
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
    const { step } = z.object({ step: z.string() }).parse(await request.json())
    const retryable = demoRetryableSteps(build)
    const position = retryable.indexOf(step)
    if (position < 0) {
      return HttpResponse.json(
        {
          error: `${step} cannot be retried for this build`,
          code: 'step_not_retryable',
        },
        { status: 409 },
      )
    }
    const retry: (typeof demoState.builds)[number] = {
      ...build,
      id: `build-demo-retry-${crypto.randomUUID().slice(0, 8)}`,
      build_number:
        Math.max(
          ...demoState.builds
            .filter((candidate) => candidate.project_id === build.project_id)
            .map((candidate) => candidate.build_number),
        ) + 1,
      status: 'queued',
      trigger_type: 'manual',
      trigger_event: 'retry',
      source_build_id: build.id,
      retry_steps: retryable.slice(position),
      step_results: undefined,
      runner_id: undefined,
      started_at: undefined,
      finished_at: undefined,
      exit_code: undefined,
      queued_at: ago(0),
      created_at: ago(0),
      updated_at: ago(0),
    }
    demoState.builds.unshift(retry)
    demoState.buildEvents[retry.id] = [
      {
        id: `evt-${retry.id}-queued`,
        build_id: retry.id,
        to_status: 'queued',
        reason: `retry of ${retry.retry_steps?.join(', ')} from build #${build.build_number}`,
        created_at: retry.created_at,
      },
    ]
    return HttpResponse.json(
      { build: withBuildContext(retry) },
      { status: 201 },
    )
  }),

  // Stream token — return 503 to trigger polling fallback in useLogStream
  demoApi.createStreamToken(async () => {
    await delay(100)
    return HttpResponse.json(
      { error: 'Demo mode: live streaming unavailable', code: 'demo_mode' },
      { status: 503 },
    )
  }),

  demoApi.getBuildLogs(async ({ params, request }) => {
    await delay(200)
    const buildId = String(params.build_id)
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

  demoApi.getBuildTestResults(async ({ params, request }) => {
    await delay(150)
    const buildId = String(params.build_id)
    const persona = getDemoPersonaFromRequest(request)
    const build = demoState.builds.find((candidate) => candidate.id === buildId)
    if (!build || !getDemoProjectRole(persona, build.project_id)) {
      return HttpResponse.json(
        { error: 'Build not found', code: 'not_found' },
        { status: 404 },
      )
    }
    return HttpResponse.json(
      demoState.buildTests[buildId] ?? { failures: [], history_window: 20 },
    )
  }),

  demoApi.listArtifacts(async ({ params, request }) => {
    await delay(150)
    const buildId = String(params.build_id)
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

  demoApi.listProjectArtifacts(async ({ params, request }) => {
    await delay(150)
    const persona = getDemoPersonaFromRequest(request)
    if (!getDemoProjectRole(persona, String(params.project_id))) {
      return HttpResponse.json(
        { error: 'Project not found', code: 'not_found' },
        { status: 404 },
      )
    }
    const projectBuildIds = demoState.builds
      .filter((build) => build.project_id === params.project_id)
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

  demoApi.listBuildArtifacts(async ({ request }) => {
    await delay(150)
    const persona = getDemoPersonaFromRequest(request)
    const body = artifactQuerySchema.parse(await request.json())
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
