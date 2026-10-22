import * as z from 'zod'
import type { BuildLogChunk, StepResult } from '@oore/client/models'

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
    const parsed = z
      .object({
        event: z.string().optional(),
        name: z.string().optional(),
        status: z.string().optional(),
        command: z.string().optional(),
      })
      .parse(JSON.parse(content.slice(prefix.length)))
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
  const allVisibleLogs: Array<BuildLogChunk> = []
  let activeStep: string | null = null

  const ensureGroup = (name: string): StepGroup => {
    const existing = groups.get(name)
    if (existing) return existing
    const created: StepGroup = { name, status: 'pending', logs: [] }
    groups.set(name, created)
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

  const stepGroups = [...groups.values()]

  return {
    stepGroups,
    stepGroupsByName: groups,
    allVisibleLogs,
    runningStepName:
      stepGroups.find((group) => group.status === 'running')?.name ?? null,
  }
}

/** Name of the step still running, read from the last step marker. */
export function currentStepName(logs: Array<BuildLogChunk>): string | null {
  for (let index = logs.length - 1; index >= 0; index--) {
    const marker = parseStepMarker(logs[index].content)
    if (marker) return marker.event === 'start' ? marker.name : null
  }
  return null
}

// oxlint-disable-next-line no-control-regex
const ANSI_PATTERN = /\u001b\[[0-9;?]*[ -/]*[@-~]/g
const OORE_PREFIX_PATTERN = /^\[oore-[a-z-]+\]\s*/

export interface FailureLine {
  step: string
  sequence: number
  lineNumber: number
  text: string
}

/**
 * The line that best explains why the build failed: the last error-looking
 * line of the failed step, else that step's last non-blank line.
 */
export function findFailureLine(
  stepGroups: Array<StepGroup>,
  allVisibleLogs: Array<BuildLogChunk>,
): FailureLine | null {
  const group = stepGroups.find(
    (candidate) =>
      (candidate.status === 'failed' || candidate.status === 'timed_out') &&
      candidate.logs.length > 0,
  )
  if (!group) return null
  const lines = group.logs.filter((chunk) => chunk.content.trim())
  const chunk =
    [...lines].reverse().find((candidate) => isErrorLine(candidate.content)) ??
    lines.at(-1)
  if (!chunk) return null
  return {
    step: group.name,
    sequence: chunk.sequence,
    lineNumber: allVisibleLogs.indexOf(chunk) + 1,
    text: chunk.content
      .replace(ANSI_PATTERN, '')
      .replace(OORE_PREFIX_PATTERN, '')
      .trim(),
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
