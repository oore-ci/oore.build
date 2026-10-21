import type {
  Build,
  BuildStatus,
  BuildInsightsDay,
  BuildInsightsResponse,
  BuildOutcomeTotals,
  DurationPercentiles,
  FailingPipelineInsights,
  Pipeline,
  ProjectBuildInsights,
} from '@oore/client/models'

const DAY = 86_400
const HISTORY_DAYS = 180

type DemoProjectName = { id: string; name: string }

const FINISHED = new Set<BuildStatus>([
  'succeeded',
  'failed',
  'timed_out',
  'canceled',
  'expired',
])

type FinishedBuild = {
  id: string
  status: BuildStatus
  projectId: string
  projectName: string
  pipelineId: string
  pipelineName: string
  buildNumber: number
  runSeconds?: number
  waitSeconds?: number
  finishedAt: number
}

/** Small deterministic PRNG so the demo history is the same on every load. */
function random(seed: number) {
  let state = seed
  return () => {
    state = (state + 0x6d2b79f5) | 0
    let t = Math.imul(state ^ (state >>> 15), 1 | state)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4_294_967_296
  }
}

function seedFor(id: string): number {
  let hash = 0
  for (const char of id) hash = (Math.imul(hash, 31) + char.charCodeAt(0)) | 0
  return hash
}

/**
 * Older builds that never appear in the build list: enough history for the
 * dashboard to draw trends and compare periods. Each pipeline gets its own
 * cadence, failure rate and build time.
 */
function syntheticHistory(
  pipelines: Array<Pipeline>,
  projects: Array<DemoProjectName>,
  now: number,
): Array<FinishedBuild> {
  const history: Array<FinishedBuild> = []
  for (const pipeline of pipelines) {
    const project = projects.find((p) => p.id === pipeline.project_id)
    if (!project) continue
    const next = random(seedFor(pipeline.id))
    const perDay = 1 + next() * 3
    const failureRate = 0.05 + next() * 0.2
    const baseSeconds = 300 + Math.round(next() * 900)
    let number = 0
    // Start two hours back so the visible demo builds stay the newest.
    for (let day = HISTORY_DAYS; day >= 0; day -= 1) {
      const weekend = new Date((now - day * DAY) * 1000).getDay() % 6 === 0
      const count = Math.floor(next() * perDay * (weekend ? 0.4 : 1.4))
      for (let index = 0; index < count; index += 1) {
        const finishedAt = now - day * DAY - 7_200 - Math.floor(next() * 50_000)
        const roll = next()
        // Builds got faster and steadier over the last month.
        const recent = day < 30
        const status: BuildStatus =
          roll < failureRate * (recent ? 0.7 : 1)
            ? 'failed'
            : roll < failureRate + 0.02
              ? 'timed_out'
              : roll < failureRate + 0.05
                ? 'canceled'
                : 'succeeded'
        number += 1
        history.push({
          id: `${pipeline.id}-history-${number}`,
          status,
          projectId: project.id,
          projectName: project.name,
          pipelineId: pipeline.id,
          pipelineName: pipeline.name,
          buildNumber: number,
          runSeconds: Math.round(
            baseSeconds * (recent ? 0.85 : 1) * (0.7 + next() * 0.7),
          ),
          waitSeconds: Math.round(next() * next() * 240),
          finishedAt,
        })
      }
    }
  }
  return history
}

function totals(builds: Array<FinishedBuild>): BuildOutcomeTotals {
  const count = (status: BuildStatus) =>
    builds.filter((build) => build.status === status).length
  const result: BuildOutcomeTotals = {
    finished: builds.length,
    succeeded: count('succeeded'),
    failed: count('failed'),
    timed_out: count('timed_out'),
    canceled: count('canceled'),
    expired: count('expired'),
  }
  const verdicts = result.succeeded + result.failed + result.timed_out
  if (verdicts > 0) result.success_rate = result.succeeded / verdicts
  return result
}

function percentiles(values: Array<number>): DurationPercentiles {
  const sorted = [...values].sort((a, b) => a - b)
  const rank = (p: number) =>
    sorted[Math.max(1, Math.ceil(p * sorted.length)) - 1]
  return {
    p50_seconds: rank(0.5),
    p90_seconds: rank(0.9),
    samples: sorted.length,
  }
}

const isFailure = (build: FinishedBuild) =>
  build.status === 'failed' || build.status === 'timed_out'

const successSeconds = (builds: Array<FinishedBuild>) =>
  builds.flatMap((build) =>
    build.status === 'succeeded' && build.runSeconds !== undefined
      ? [build.runSeconds]
      : [],
  )

function groupBy(
  builds: Array<FinishedBuild>,
  key: 'projectId' | 'pipelineId',
) {
  const groups = new Map<string, Array<FinishedBuild>>()
  for (const build of builds) {
    groups.set(build[key], [...(groups.get(build[key]) ?? []), build])
  }
  return [...groups.values()]
}

/** Mirrors `GET /v1/builds/insights` over the demo's builds and history. */
export function demoBuildInsights({
  builds,
  days,
  now,
  offsetMinutes,
  pipelines,
  projects,
}: {
  builds: Array<Build>
  days: number
  now: number
  offsetMinutes: number
  pipelines: Array<Pipeline>
  projects: Array<DemoProjectName>
}): BuildInsightsResponse {
  const offset = offsetMinutes * 60
  const local = now + offset
  const since =
    local - (((local % DAY) + DAY) % DAY) - offset - (days - 1) * DAY
  const previousSince = since - days * DAY

  const visibleProjects = new Set(builds.map((build) => build.project_id))
  const finished: Array<FinishedBuild> = [
    ...syntheticHistory(pipelines, projects, now).filter((build) =>
      visibleProjects.has(build.projectId),
    ),
    ...builds.flatMap((build) => {
      const finishedAt = build.finished_at
      if (finishedAt == null || !FINISHED.has(build.status)) return []
      const startedAt = build.started_at
      return [
        {
          id: build.id,
          status: build.status,
          projectId: build.project_id,
          projectName:
            projects.find((p) => p.id === build.project_id)?.name ??
            build.project_id,
          pipelineId: build.pipeline_id,
          pipelineName:
            pipelines.find((p) => p.id === build.pipeline_id)?.name ??
            build.pipeline_id,
          buildNumber: build.build_number,
          runSeconds: startedAt == null ? undefined : finishedAt - startedAt,
          waitSeconds:
            startedAt == null ? undefined : startedAt - build.queued_at,
          finishedAt,
        },
      ]
    }),
  ]
    .filter((build) => build.finishedAt >= previousSince)
    .sort((a, b) => a.finishedAt - b.finishedAt)

  const current = finished.filter((build) => build.finishedAt >= since)
  const previous = finished.filter((build) => build.finishedAt < since)

  const daily: Array<BuildInsightsDay> = Array.from(
    { length: days },
    (_, index) => {
      const dayStart = since + index * DAY
      const dayBuilds = current.filter(
        (build) =>
          build.finishedAt >= dayStart && build.finishedAt < dayStart + DAY,
      )
      return {
        day_start: dayStart,
        succeeded: dayBuilds.filter((b) => b.status === 'succeeded').length,
        failed: dayBuilds.filter(isFailure).length,
        other: dayBuilds.filter(
          (b) => b.status === 'canceled' || b.status === 'expired',
        ).length,
        p50_duration_seconds: percentiles(successSeconds(dayBuilds))
          .p50_seconds,
      }
    },
  )

  const projectInsights: Array<ProjectBuildInsights> = groupBy(
    current,
    'projectId',
  )
    .map((group) => {
      const last = group[group.length - 1]
      return {
        project_id: last.projectId,
        project_name: last.projectName,
        totals: totals(group),
        duration: percentiles(successSeconds(group)),
        last_status: last.status,
        last_finished_at: last.finishedAt,
      }
    })
    .sort(
      (a, b) =>
        b.totals.finished - a.totals.finished ||
        a.project_name.localeCompare(b.project_name),
    )

  const failingPipelines: Array<FailingPipelineInsights> = groupBy(
    current,
    'pipelineId',
  )
    .flatMap((group) => {
      const lastFailed = [...group].reverse().find(isFailure)
      if (!lastFailed) return []
      let streak = 0
      for (const build of [...group].reverse()) {
        if (build.status === 'canceled' || build.status === 'expired') continue
        if (!isFailure(build)) break
        streak += 1
      }
      return [
        {
          pipeline_id: lastFailed.pipelineId,
          pipeline_name: lastFailed.pipelineName,
          project_id: lastFailed.projectId,
          project_name: lastFailed.projectName,
          failures: group.filter(isFailure).length,
          finished: group.length,
          current_streak: streak,
          last_failed_build_id: lastFailed.id,
          last_failed_build_number: lastFailed.buildNumber,
          last_failed_at: lastFailed.finishedAt,
        },
      ]
    })
    .sort(
      (a, b) =>
        b.current_streak - a.current_streak ||
        b.failures - a.failures ||
        b.last_failed_at - a.last_failed_at,
    )
    .slice(0, 5)

  const active = builds.filter((build) =>
    ['queued', 'scheduled', 'assigned', 'running'].includes(build.status),
  )
  const waiting = active.filter(
    (build) =>
      (build.status === 'queued' || build.status === 'scheduled') &&
      build.approval?.state !== 'pending',
  )

  return {
    window_days: days,
    since,
    totals: totals(current),
    previous_totals: totals(previous),
    duration: percentiles(successSeconds(current)),
    previous_duration: percentiles(successSeconds(previous)),
    queue_wait: percentiles(
      current.flatMap((build) =>
        build.waitSeconds === undefined ? [] : [build.waitSeconds],
      ),
    ),
    daily,
    queue: {
      waiting: waiting.length,
      awaiting_approval: active.filter(
        (build) => build.approval?.state === 'pending',
      ).length,
      running: active.filter(
        (build) => build.status === 'assigned' || build.status === 'running',
      ).length,
      oldest_waiting_since:
        waiting.length > 0
          ? Math.min(...waiting.map((build) => build.queued_at))
          : undefined,
    },
    projects: projectInsights,
    failing_pipelines: failingPipelines,
  }
}
