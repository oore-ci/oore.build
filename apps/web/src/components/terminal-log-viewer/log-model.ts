import type { BuildLogChunk, StepResult } from '@/lib/types'

import type { StepGroup } from './types'

interface StepMarker {
  event: 'start' | 'end'
  name: string
  status?: string
  command?: string
}

export const ERROR_PATTERN =
  /\b(error|ERROR|FAILED|FAILURE|fatal|FATAL|exception|EXCEPTION|panic|PANIC)\b/

function parseStepMarker(content: string): StepMarker | null {
  const prefix = '[oore-step] '
  if (!content.startsWith(prefix)) return null

  try {
    const parsed = JSON.parse(content.slice(prefix.length)) as {
      event?: string
      name?: string
      status?: string
      command?: string
    }
    if (
      (parsed.event === 'start' || parsed.event === 'end') &&
      parsed.name?.trim()
    ) {
      return {
        event: parsed.event,
        name: parsed.name.trim(),
        status: parsed.status?.trim(),
        command: parsed.command?.trim(),
      }
    }
  } catch {
    // Invalid marker-like lines are ordinary log output.
  }
  return null
}

export function groupLogs(
  logs: Array<BuildLogChunk>,
  stepResults: Array<StepResult>,
) {
  const groups = new Map<string, StepGroup>()
  const order: Array<string> = []
  const allVisibleLogs: Array<BuildLogChunk> = []
  let activeStep: string | null = null

  const ensureGroup = (name: string): StepGroup => {
    const existing = groups.get(name)
    if (existing) return existing
    const created: StepGroup = { name, status: 'pending', logs: [] }
    groups.set(name, created)
    order.push(name)
    return created
  }

  for (const chunk of logs) {
    const marker = parseStepMarker(chunk.content)
    if (marker) {
      const group = ensureGroup(marker.name)
      if (marker.event === 'start') {
        group.status = 'running'
        if (marker.command) group.command = marker.command
        activeStep = marker.name
      } else {
        group.status = marker.status ?? 'succeeded'
        activeStep = activeStep === marker.name ? null : activeStep
      }
      continue
    }

    allVisibleLogs.push(chunk)
    if (activeStep) ensureGroup(activeStep).logs.push(chunk)
  }

  for (const result of stepResults) {
    const group = ensureGroup(result.name)
    group.status = result.status
    group.durationMs = result.duration_ms
  }

  const stepGroups = order.flatMap((name) => {
    const group = groups.get(name)
    return group ? [group] : []
  })

  return {
    stepGroups,
    stepGroupsByName: groups,
    allVisibleLogs,
    runningStepName:
      stepGroups.find((group) => group.status === 'running')?.name ?? null,
  }
}

export function defaultSelectedStep(
  stepGroups: Array<StepGroup>,
  runningStepName: string | null,
) {
  if (runningStepName) return runningStepName
  return (
    stepGroups.find(
      (group) => group.status === 'failed' && group.logs.length > 0,
    )?.name ??
    stepGroups.find((group) => group.logs.length > 0)?.name ??
    'all'
  )
}

export function findFirstErrorIndex(lines: Array<BuildLogChunk>): number {
  return lines.findIndex(
    (chunk) => chunk.stream === 'stderr' || ERROR_PATTERN.test(chunk.content),
  )
}

export function formatDuration(seconds: number): string {
  if (seconds < 60) return `${Math.round(seconds)}s`
  const mins = Math.floor(seconds / 60)
  const secs = Math.round(seconds % 60)
  if (mins < 60) return `${mins}m ${secs}s`
  return `${Math.floor(mins / 60)}h ${mins % 60}m`
}

export function stepStatusVariant(status: string) {
  const normalized = status.trim().toLowerCase()
  if (normalized === 'running') return 'info' as const
  if (normalized === 'succeeded') return 'success' as const
  if (
    normalized === 'failed' ||
    normalized === 'canceled' ||
    normalized === 'timed_out'
  ) {
    return 'destructive' as const
  }
  return 'outline' as const
}
