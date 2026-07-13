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
    cleanup()
    if (!enabled || !baseUrl || !token) {
      updateStream({ isStreaming: false })
      return cleanup
    }

    updateStream('reset')
    logsBySequenceRef.current = new Map()
    orderedLogsRef.current = []
    lastSequenceRef.current = -1

    const abort = new AbortController()
    abortRef.current = abort

    startPolling()

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
        const es = new EventSource(streamUrl)
        eventSourceRef.current = es

        es.addEventListener('open', () => {
          // SSE is healthy, so suspend polling reconciliation until disconnect.
          stopPolling()
          updateStream({ isStreaming: true })
        })

        es.addEventListener('log', (event: MessageEvent) => {
          try {
            const chunk = JSON.parse(event.data as string) as BuildLogChunk
            appendLogs([chunk])
          } catch {
            // Ignore malformed chunks.
          }
        })

        es.addEventListener('done', () => {
          updateStream({ isStreaming: false, isDone: true })
          es.close()
          eventSourceRef.current = null
          void pollOnce()
          stopPolling()
          onDone?.()
        })

        es.addEventListener('error', () => {
          es.close()
          eventSourceRef.current = null
          updateStream({ isStreaming: false })
          startPolling()
        })
      } catch {
        // Polling remains active if EventSource cannot be constructed.
      }
    })()

    return cleanup
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
