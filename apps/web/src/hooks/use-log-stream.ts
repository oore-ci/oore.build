import { useCallback, useEffect, useRef, useState } from 'react'

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
  error: string | null
}

interface UseLogStreamOptions {
  onDone?: () => void
}

const POLL_INTERVAL_MS = 2500
const POLL_BACKFILL_WINDOW = 500

export function useLogStream(
  buildId: string,
  enabled: boolean,
  options?: UseLogStreamOptions,
): UseLogStreamResult {
  const onDone = options?.onDone
  const [logs, setLogs] = useState<Array<BuildLogChunk>>([])
  const [isStreaming, setIsStreaming] = useState(false)
  const [isDone, setIsDone] = useState(false)
  const [error, setError] = useState<string | null>(null)

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
    setLogs(merged.logs)
  }, [])

  const pollOnce = useCallback(async () => {
    if (!baseUrl || !token) return
    const after = Math.max(-1, lastSequenceRef.current - POLL_BACKFILL_WINDOW)
    try {
      const response = await getBuildLogs(baseUrl, token, buildId, {
        after_sequence: after >= 0 ? after : undefined,
      })
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
    if (!enabled || !baseUrl || !token) {
      cleanup()
      return
    }

    setLogs([])
    setIsStreaming(false)
    setIsDone(false)
    setError(null)
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
        if (!abort.signal.aborted) {
          setError('Live stream unavailable. Using polling fallback.')
        }
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
          setIsStreaming(true)
          setError(null)
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
          setIsStreaming(false)
          setIsDone(true)
          es.close()
          eventSourceRef.current = null
          void pollOnce()
          stopPolling()
          onDone?.()
        })

        es.addEventListener('error', () => {
          es.close()
          eventSourceRef.current = null
          setIsStreaming(false)
          setError('Live stream disconnected. Continuing with polling.')
          startPolling()
        })
      } catch {
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- abort may happen concurrently
        if (!abort.signal.aborted) {
          setError('Failed to connect live stream. Using polling fallback.')
        }
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

  return { logs, isStreaming, isDone, error }
}
