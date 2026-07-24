import type { BuildLogChunk, StepResult } from '@/lib/types'

import type { StepGroup } from './types'

interface StepMarker {
  event: 'start' | 'end'
  name: string
  status?: string
  command?: string
}

const ERROR_PATTERNS = [
  /^(?:error|fatal|failure|exception|panic)(?:\b|:)/i,
  /(?:^|\s)(?:error|fatal|exception|panic):\s/i,
  /\b(?:build failed|failed with|execution failed|uncaught exception)\b/i,
]

export function isErrorLine(content: string): boolean {
  const normalized = content.trim()
  if (!normalized || normalized.startsWith('$ ')) return false
  return ERROR_PATTERNS.some((pattern) => pattern.test(normalized))
}

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
    )?.name ?? 'all'
  )
}

export function findFirstErrorIndex(lines: Array<BuildLogChunk>): number {
  return lines.findIndex((chunk) => isErrorLine(chunk.content))
}
