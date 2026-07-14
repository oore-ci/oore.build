import type { BuildLogChunk, StepResult } from '@/lib/types'

export interface TerminalLogViewerProps {
  logs: Array<BuildLogChunk>
  stepResults: Array<StepResult>
  isStreaming: boolean
  fillAvailableHeight?: boolean
  isLoading?: boolean
  logsUnavailable?: boolean
  isTerminal?: boolean
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
