import type { Build } from '@oore/client/models'

/** Fewer finished runs than this make a median too noisy to compare with. */
export const MIN_MEDIAN_SAMPLES = 3

/**
 * Median duration of each step across `builds`, leaving out `excludeBuildId`,
 * reruns and step retries, and steps that did not succeed. Steps with fewer than
 * `MIN_MEDIAN_SAMPLES` runs are left out.
 */
export function stepMedians(
  builds: ReadonlyArray<Build>,
  excludeBuildId?: string,
): Map<string, number> {
  const samples = new Map<string, Array<number>>()
  for (const build of builds) {
    // Reruns and step retries reuse earlier work, so their times are not
    // typical runs.
    if (
      build.id === excludeBuildId ||
      build.source_build_id ||
      build.retry_steps?.length
    ) {
      continue
    }
    for (const step of build.step_results ?? []) {
      if (step.status !== 'succeeded' || step.duration_ms < 0) continue
      const durations = samples.get(step.name) ?? []
      durations.push(step.duration_ms)
      samples.set(step.name, durations)
    }
  }
  const medians = new Map<string, number>()
  for (const [name, durations] of samples) {
    if (durations.length < MIN_MEDIAN_SAMPLES) continue
    durations.sort((left, right) => left - right)
    const middle = Math.floor(durations.length / 2)
    medians.set(
      name,
      durations.length % 2
        ? durations[middle]
        : Math.round((durations[middle - 1] + durations[middle]) / 2),
    )
  }
  return medians
}

/**
 * How a step's time compares with its median: `slower` or `faster` once it
 * is off by at least 20% and 5 seconds, otherwise `typical`.
 */
export function stepTimingTrend(
  durationMs: number,
  medianMs: number,
): 'faster' | 'slower' | 'typical' {
  const delta = durationMs - medianMs
  if (Math.abs(delta) < 5_000 || Math.abs(delta) < medianMs * 0.2) {
    return 'typical'
  }
  return delta > 0 ? 'slower' : 'faster'
}
