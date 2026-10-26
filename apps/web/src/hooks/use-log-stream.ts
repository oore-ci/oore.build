import { useEffect, useEffectEvent, useReducer, useState } from 'react'
import { streamBuildLogEvents } from '@oore/client/logs'
import type { BuildLogChunk } from '@oore/client/models'
import { getBuildLogs } from '@oore/client/operations'

import {
  createLogFrameBatcher,
  mergeBuildLogChunks,
} from '@/lib/log-stream-utils'
import { useApiContext } from '@/hooks/use-api-context'
import { createWebOoreClient } from '@/lib/api-client/client'

interface LogStreamState {
  logs: Array<BuildLogChunk>
  isStreaming: boolean
  isDone: boolean
  /** The live stream was unavailable or dropped; lines arrive by polling. */
  isPolling: boolean
}

interface UseLogStreamResult extends LogStreamState {
  /** Try the live stream again after it dropped. */
  reconnect: () => void
}

interface UseLogStreamOptions {
  onDone?: () => void
}

const POLL_INTERVAL_MS = 2500
// Re-read a few recent lines in case they committed out of order.
const POLL_BACKFILL_WINDOW = 50
const POLL_PAGE_SIZE = 5000

type StreamAction = Partial<LogStreamState> | 'reset'

const initialStreamState: LogStreamState = {
  logs: [],
  isStreaming: false,
  isDone: false,
  isPolling: false,
}

function streamReducer(
  state: LogStreamState,
  action: StreamAction,
): LogStreamState {
  return action === 'reset' ? initialStreamState : { ...state, ...action }
}

export function useLogStream(
  buildId: string,
  enabled: boolean,
  options?: UseLogStreamOptions,
): UseLogStreamResult {
  const onDone = useEffectEvent(() => options?.onDone?.())
  const [stream, updateStream] = useReducer(streamReducer, initialStreamState)
  const [attempt, setAttempt] = useState(0)
  const { baseUrl, token } = useApiContext()

  useEffect(() => {
    if (!enabled || !baseUrl || !token) {
      updateStream({ isStreaming: false })
      return
    }

    const client = createWebOoreClient({ baseUrl, token })
    const abort = new AbortController()
    const logsBySequence = new Map<number, BuildLogChunk>()
    let orderedLogs: Array<BuildLogChunk> = []
    let lastSequence = -1
    let pollingTimer: ReturnType<typeof setInterval> | undefined

    function appendLogs(chunks: Array<BuildLogChunk>) {
      if (abort.signal.aborted || chunks.length === 0) return

      const merged = mergeBuildLogChunks(orderedLogs, logsBySequence, chunks)
      if (!merged.changed) return

      orderedLogs = merged.logs
      lastSequence = merged.lastSequence
      updateStream({ logs: merged.logs })
    }

    let polling = false
    // Page until caught up so a long or verbose log never falls behind.
    async function pollOnce() {
      if (polling) return
      polling = true
      try {
        let after = Math.max(-1, lastSequence - POLL_BACKFILL_WINDOW)
        for (;;) {
          const response = await getBuildLogs({
            client,
            path: { build_id: buildId },
            query: {
              after_sequence: after >= 0 ? after : undefined,
              limit: POLL_PAGE_SIZE,
            },
            signal: abort.signal,
          })
          appendLogs(response.logs)
          if (response.logs.length < POLL_PAGE_SIZE) break
          after = response.logs.at(-1)?.sequence ?? after
        }
      } catch {
        // Retry on the next interval.
      } finally {
        polling = false
      }
    }

    function startPolling() {
      if (pollingTimer) return
      updateStream({ isPolling: true })
      void pollOnce()
      pollingTimer = setInterval(() => void pollOnce(), POLL_INTERVAL_MS)
    }

    function stopPolling() {
      if (!pollingTimer) return
      clearInterval(pollingTimer)
      pollingTimer = undefined
    }

    updateStream('reset')

    const logBatcher = createLogFrameBatcher(appendLogs)
    // Read what already exists while the live stream connects.
    void pollOnce()

    void (async () => {
      try {
        for await (const event of streamBuildLogEvents({
          client,
          path: { build_id: buildId },
          signal: abort.signal,
        })) {
          if (abort.signal.aborted) return
          stopPolling()
          updateStream({ isStreaming: true, isPolling: false })

          if (event.type === 'log') {
            logBatcher.enqueue(event.chunk)
          } else if (event.type === 'done') {
            logBatcher.flush()
            updateStream({ isStreaming: false, isDone: true, isPolling: false })
            void pollOnce()
            onDone()
            return
          }
        }
      } catch {
        // Polling is the supported fallback when SSE is unavailable.
      }

      if (!abort.signal.aborted) {
        updateStream({ isStreaming: false })
        startPolling()
      }
    })()

    return () => {
      abort.abort()
      logBatcher.cancel()
      stopPolling()
    }
  }, [enabled, baseUrl, token, buildId, attempt])

  return { ...stream, reconnect: () => setAttempt((value) => value + 1) }
}
