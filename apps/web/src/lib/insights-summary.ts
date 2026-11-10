import type { BuildInsightsResponse } from '@oore/client/models'

/** Below this success rate, or this far under last period, builds are off. */
const LOW_SUCCESS_RATE = 0.8
const DROP_POINTS = 0.1
/** An app needs this many finished builds before its rate means anything. */
const MIN_APP_BUILDS = 3
const LOW_APP_SUCCESS_RATE = 0.5

const percent = (rate: number) => `${Math.round(rate * 100)}%`

/** One sentence when builds overall are going worse than usual, else null. */
export function buildHealthLine(
  insights: BuildInsightsResponse,
): string | null {
  const rate = insights.totals.success_rate
  if (rate == null || insights.totals.finished === 0) return null
  const previous = insights.previous_totals.success_rate
  const dropped = previous != null && previous - rate >= DROP_POINTS
  if (rate >= LOW_SUCCESS_RATE && !dropped) return null
  const days = insights.window_days
  return dropped
    ? `${percent(rate)} of builds succeeded in the last ${days} days, down from ${percent(previous)}.`
    : `${percent(rate)} of builds succeeded in the last ${days} days.`
}

/** One sentence naming apps that fail most of their builds, else null. */
export function appHealthLine(insights: BuildInsightsResponse): string | null {
  const struggling = insights.projects.filter(
    (project) =>
      project.totals.finished >= MIN_APP_BUILDS &&
      (project.totals.success_rate ?? 1) < LOW_APP_SUCCESS_RATE,
  )
  if (struggling.length === 0) return null
  const [first] = struggling
  if (struggling.length === 1) {
    return `${first.project_name} failed ${first.totals.failed} of its last ${first.totals.finished} builds.`
  }
  return `${first.project_name} and ${struggling.length - 1} other app${struggling.length === 2 ? '' : 's'} fail most of their builds.`
}

/** One sentence when a store reports an app past or near a threshold. */
export function storeHealthLine({
  breached,
  warning,
}: {
  breached: number
  warning: number
}): string | null {
  if (breached > 0) {
    return `${breached} app${breached === 1 ? ' is' : 's are'} past a store's crash or ANR threshold.`
  }
  if (warning > 0) {
    return `${warning} app${warning === 1 ? ' is' : 's are'} close to a store's crash or ANR threshold.`
  }
  return null
}
