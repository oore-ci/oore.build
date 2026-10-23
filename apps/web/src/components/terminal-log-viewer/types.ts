import type {
  BuildLogChunk,
  BuildStatus,
  StepResult,
} from '@oore/client/models'

export interface TerminalLogViewerProps {
  logs: Array<BuildLogChunk>
  stepResults: Array<StepResult>
  isStreaming: boolean
  fillAvailableHeight?: boolean
  isLoading?: boolean
  logsUnavailable?: boolean
  onRetryLogs?: () => void
  isTerminal?: boolean
  /** Picks the empty-state copy and when a dropped stream is worth showing. */
  buildStatus?: BuildStatus
  /** Names the downloaded file `build-<n>.log`. */
  buildNumber?: number
  /** The live stream dropped and lines arrive by polling. */
  isPolling?: boolean
  onReconnect?: () => void
  /** Select a step and scroll to one of its lines; a new `key` repeats it. */
  focusRequest?: LogFocusRequest | null
  /** Typical duration in ms of each step, by step name, to compare against. */
  stepMedians?: ReadonlyMap<string, number>
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
