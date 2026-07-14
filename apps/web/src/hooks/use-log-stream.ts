import { useCallback, useEffect, useReducer, useRef } from 'react'

import type { BuildLogChunk } from '@/lib/types'
import { createStreamToken, getBuildLogs } from '@/lib/api'
import { mergeBuildLogChunks } from '@/lib/log-stream-utils'
import { useAuthStore } from '@/stores/auth-store'
import { useActiveInstance } from '@/stores/instance-store'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'

interface UseLogStreamResult {
  logs: Array<BuildLogChunk>
  isStreaming: boolean
  isDone: boolean
}

interface UseLogStreamOptions {
  onDone?: () => void
}

const POLL_INTERVAL_MS = 2500
const POLL_BACKFILL_WINDOW = 500

interface LogEventSourceHandlers {
  open: EventListener
  log: EventListener
  done: EventListener
  error: EventListener
}

function openEventSource(
  url: string,
  handlers: LogEventSourceHandlers,
): EventSource {
  const eventSource = new EventSource(url)
  eventSource.addEventListener('open', handlers.open)
  eventSource.addEventListener('log', handlers.log)
  eventSource.addEventListener('done', handlers.done)
  eventSource.addEventListener('error', handlers.error)
  return eventSource
}

type StreamState = UseLogStreamResult
type StreamAction = Partial<StreamState> | 'reset'

const initialStreamState: StreamState = {
  logs: [],
  isStreaming: false,
  isDone: false,
}

function streamReducer(state: StreamState, action: StreamAction): StreamState {
  return action === 'reset' ? initialStreamState : { ...state, ...action }
}

export function useLogStream(
  buildId: string,
  enabled: boolean,
  options?: UseLogStreamOptions,
): UseLogStreamResult {
  const onDone = options?.onDone
  const [stream, updateStream] = useReducer(streamReducer, initialStreamState)

  const instance = useActiveInstance()
  const baseUrl = resolveInstanceApiBaseUrl(instance)
  const token = useAuthStore((s) => s.token)

  const eventSourceRef = useRef<EventSource | null>(null)
  const pollingRef = useRef<ReturnType<typeof setInterval> | null>(null)
  const abortRef = useRef<AbortController | null>(null)

  const logsBySequenceRef = useRef<Map<number, BuildLogChunk>>(new Map())
  const orderedLogsRef = useRef<Array<BuildLogChunk>>([])
  const lastSequenceRef = useRef(-1)

  const appendLogs = useCallback((chunks: Array<BuildLogChunk>) => {
    if (chunks.length === 0) return

    const merged = mergeBuildLogChunks(
      orderedLogsRef.current,
      logsBySequenceRef.current,
      chunks,
    )
    if (!merged.changed) return

    orderedLogsRef.current = merged.logs
    lastSequenceRef.current = merged.lastSequence
    updateStream({ logs: merged.logs })
  }, [])

  const pollOnce = useCallback(async () => {
    if (!baseUrl || !token) return
    const after = Math.max(-1, lastSequenceRef.current - POLL_BACKFILL_WINDOW)
    try {
      const response = await getBuildLogs(
        baseUrl,
        token,
        buildId,
        { after_sequence: after >= 0 ? after : undefined },
        { signal: abortRef.current?.signal },
      )
      appendLogs(response.logs)
    } catch {
      // Retry on next interval.
    }
  }, [baseUrl, token, buildId, appendLogs])

  const stopPolling = useCallback(() => {
    if (pollingRef.current) {
      clearInterval(pollingRef.current)
      pollingRef.current = null
    }
  }, [])

  const startPolling = useCallback(() => {
    if (!baseUrl || !token || pollingRef.current) return
    void pollOnce()
    pollingRef.current = setInterval(() => {
      void pollOnce()
    }, POLL_INTERVAL_MS)
  }, [baseUrl, token, pollOnce])

  const cleanup = useCallback(() => {
    if (abortRef.current) {
      abortRef.current.abort()
      abortRef.current = null
    }
    if (eventSourceRef.current) {
      eventSourceRef.current.close()
      eventSourceRef.current = null
    }
    stopPolling()
  }, [stopPolling])

  useEffect(() => {
    let eventSource: EventSource | null = null

    cleanup()
    if (!enabled || !baseUrl || !token) {
      updateStream({ isStreaming: false })
      return
    }

    updateStream('reset')
    logsBySequenceRef.current = new Map()
    orderedLogsRef.current = []
    lastSequenceRef.current = -1

    const abort = new AbortController()
    abortRef.current = abort

    startPolling()

    const handleOpen = () => {
      // SSE is healthy, so suspend polling reconciliation until disconnect.
      stopPolling()
      updateStream({ isStreaming: true })
    }

    const handleLog = (event: Event) => {
      try {
        const chunk = JSON.parse((event as MessageEvent).data as string) as BuildLogChunk
        appendLogs([chunk])
      } catch {
        // Ignore malformed chunks.
      }
    }

    const handleDone = () => {
      updateStream({ isStreaming: false, isDone: true })
      eventSource?.close()
      eventSourceRef.current = null
      void pollOnce()
      stopPolling()
      onDone?.()
    }

    const handleError = () => {
      eventSource?.close()
      eventSourceRef.current = null
      updateStream({ isStreaming: false })
      startPolling()
    }

    void (async () => {
      let streamToken: string
      try {
        const response = await createStreamToken(baseUrl, token, buildId)
        streamToken = response.token
      } catch {
        // Polling is already active and is a supported transport fallback.
        return
      }

      if (abort.signal.aborted) return

      const streamUrl =
        `${baseUrl}/v1/builds/${buildId}/logs/stream?token=` +
        encodeURIComponent(streamToken)

      try {
        eventSource = openEventSource(streamUrl, {
          open: handleOpen,
          log: handleLog,
          done: handleDone,
          error: handleError,
        })
        const es = eventSource
        eventSourceRef.current = es
      } catch {
        // Polling remains active if EventSource cannot be constructed.
      }
    })()

    return () => {
      abortRef.current?.abort()
      abortRef.current = null
      eventSource?.removeEventListener('open', handleOpen)
      eventSource?.removeEventListener('log', handleLog)
      eventSource?.removeEventListener('done', handleDone)
      eventSource?.removeEventListener('error', handleError)
      eventSource?.close()
      eventSourceRef.current = null
      if (pollingRef.current) {
        clearInterval(pollingRef.current)
        pollingRef.current = null
      }
    }
  }, [
    enabled,
    baseUrl,
    token,
    buildId,
    appendLogs,
    startPolling,
    stopPolling,
    pollOnce,
    cleanup,
    onDone,
  ])

  return stream
}
