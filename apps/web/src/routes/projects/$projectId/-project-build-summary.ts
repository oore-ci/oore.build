import type { BuildStatus, ListBuildsResponse } from '@oore/client/models'

export interface LastPipelineBuild {
  buildId: string
  buildNumber: number
  status: BuildStatus
  time: number
}

export const EMPTY_LAST_BUILD_BY_PIPELINE: ReadonlyMap<
  string,
  LastPipelineBuild
> = new Map()

/** Total build count plus each pipeline's newest build in the recent page. */
export function selectProjectBuildSummary({
  builds,
  total,
}: ListBuildsResponse) {
  const lastBuildByPipeline = new Map<string, LastPipelineBuild>()

  for (const build of builds) {
    if (build.pipeline_id && !lastBuildByPipeline.has(build.pipeline_id)) {
      lastBuildByPipeline.set(build.pipeline_id, {
        buildId: build.id,
        buildNumber: build.build_number,
        status: build.status,
        time: build.queued_at,
      })
    }
  }

  return { buildCount: total, lastBuildByPipeline, builds }
}
