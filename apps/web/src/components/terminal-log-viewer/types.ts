import type { BuildLogChunk, StepResult } from '@oore/client/models'

export interface TerminalLogViewerProps {
  logs: Array<BuildLogChunk>
  stepResults: Array<StepResult>
  isStreaming: boolean
  fillAvailableHeight?: boolean
  isLoading?: boolean
  logsUnavailable?: boolean
  onRetryLogs?: () => void
  isTerminal?: boolean
  /** Select a step and scroll to one of its lines; a new `key` repeats it. */
  focusRequest?: LogFocusRequest | null
}

export interface LogFocusRequest {
  step: string
  sequence: number
  key: number
}

export interface StepGroup {
  name: string
  status: string
  command?: string
  durationMs?: number
  logs: Array<BuildLogChunk>
}

export interface SelectedStepMeta {
  command?: string
}
