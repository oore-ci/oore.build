import type { AnalyzerCounts } from '@oore/client/models'

export function plural(count: number, one: string, many: string) {
  return `${count} ${count === 1 ? one : many}`
}

export function totalDiagnostics(counts: AnalyzerCounts) {
  return counts.errors + counts.warnings + counts.infos
}

/** A count's change from the baseline, such as `+2`, `−1` or `No change`. */
export function formatCountChange(count: number, baseline: number) {
  const delta = count - baseline
  if (delta === 0) return 'No change'
  return delta > 0 ? `+${delta}` : `−${Math.abs(delta)}`
}

/** The analyzer card's title: the most severe counts first. */
export function analyzerTitle(counts: AnalyzerCounts) {
  if (totalDiagnostics(counts) === 0) return 'No issues found'
  const parts = [
    counts.errors ? plural(counts.errors, 'error', 'errors') : null,
    counts.warnings ? plural(counts.warnings, 'warning', 'warnings') : null,
    counts.infos ? plural(counts.infos, 'info', 'infos') : null,
  ].filter((part): part is string => part !== null)
  return parts.join(', ')
}

/** The count shown next to the tab name, if any. */
export function analyzerTabCount(counts: AnalyzerCounts) {
  if (counts.errors) return plural(counts.errors, 'error', 'errors')
  if (counts.warnings) return plural(counts.warnings, 'warning', 'warnings')
  return null
}
