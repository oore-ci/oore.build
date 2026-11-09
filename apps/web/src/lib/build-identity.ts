import type { Build } from '@oore/client/models'

/** The app version a build makes, as stores show it: `2.5.1 (146)`. */
export function buildVersionLabel(
  build: Pick<Build, 'version_name' | 'version_code'>,
): string | null {
  const name = build.version_name?.trim()
  const code = build.version_code
  if (name && code) return `${name} (${code})`
  if (name) return name
  return null
}

/**
 * What a build is, in the words people use: version, commit title and
 * author. Empty when the runner has not reported them yet.
 */
export function buildIdentityParts(
  build: Pick<
    Build,
    'version_name' | 'version_code' | 'commit_title' | 'commit_author'
  >,
): Array<string> {
  return [
    buildVersionLabel(build),
    build.commit_title?.trim(),
    build.commit_author?.trim(),
  ].filter((part): part is string => !!part)
}

function median(values: Array<number>): number {
  const sorted = [...values].sort((left, right) => left - right)
  const middle = Math.floor(sorted.length / 2)
  return sorted.length % 2
    ? sorted[middle]
    : Math.round((sorted[middle - 1] + sorted[middle]) / 2)
}

/** Fewer typical runs than this make a time estimate a guess. */
const MIN_PROGRESS_SAMPLES = 2

export interface BuildProgress {
  /** 1-based position of the running step, or 0 when it is not known. */
  step: number
  /** Steps a typical run of this pipeline has. */
  total: number
  /** Typical run time of this pipeline, in seconds. */
  typicalSeconds: number
  /** Seconds left at the typical pace; negative once it runs long. */
  remainingSeconds: number
}

/**
 * How far a running build is, from this pipeline's recent successful runs:
 * which step of how many, and about how long is left at the median pace.
 * Null without enough full runs to compare with.
 */
export function buildProgress({
  history,
  buildId,
  currentStep,
  elapsedSeconds,
}: {
  history: ReadonlyArray<Build>
  buildId: string
  currentStep: string | null
  elapsedSeconds: number
}): BuildProgress | null {
  const runs = history.filter(
    (build) =>
      build.id !== buildId &&
      build.status === 'succeeded' &&
      !build.source_build_id &&
      !build.retry_steps?.length &&
      build.started_at &&
      build.finished_at &&
      build.step_results?.length,
  )
  if (runs.length < MIN_PROGRESS_SAMPLES) return null
  const steps = (runs[0].step_results ?? []).map((step) => step.name)
  const typicalSeconds = median(
    runs.map((build) => (build.finished_at ?? 0) - (build.started_at ?? 0)),
  )
  return {
    step: currentStep ? steps.indexOf(currentStep) + 1 : 0,
    total: steps.length,
    typicalSeconds,
    remainingSeconds: typicalSeconds - elapsedSeconds,
  }
}

/** "about 4 min left", in the words a progress line uses. */
export function remainingLabel(remainingSeconds: number): string {
  if (remainingSeconds <= 0) return 'taking longer than usual'
  if (remainingSeconds < 60) return 'under a minute left'
  return `about ${Math.round(remainingSeconds / 60)} min left`
}

/** Builds one webhook delivery started within this many seconds of each other. */
const SAME_PUSH_WINDOW_SECONDS = 120

/**
 * The first build of each push that fanned out to several pipelines or
 * workflows, by the id of every later build of that push. A push is one
 * webhook trigger of one commit and ref in one app.
 */
export function samePushLeaders(
  builds: ReadonlyArray<Build>,
): Map<string, Build> {
  const leaders = new Map<string, Build>()
  const ordered = [...builds]
    .filter((build) => build.trigger_type === 'webhook' && build.commit_sha)
    .sort((left, right) => left.created_at - right.created_at)
  const firstOfPush = new Map<string, Build>()
  for (const build of ordered) {
    const key = [
      build.project_id,
      build.commit_sha,
      build.trigger_ref ?? build.branch ?? '',
    ].join('\u0000')
    const first = firstOfPush.get(key)
    if (
      first &&
      build.created_at - first.created_at <= SAME_PUSH_WINDOW_SECONDS &&
      (first.pipeline_id !== build.pipeline_id ||
        first.workflow !== build.workflow)
    ) {
      leaders.set(build.id, first)
    } else {
      firstOfPush.set(key, build)
    }
  }
  return leaders
}
