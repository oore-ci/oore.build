import { useEffect } from 'react'
import { streamLiveEvents } from '@oore/client/events'

import { useApiContext } from '@/hooks/use-api-context'
import { createWebOoreClient } from '@/lib/api-client/client'
import { queryClient } from '@/lib/query-client'
import {
  ALL_LIVE_TAGS,
  createTagBatcher,
  queryHasTag,
  reconnectDelay,
  tagsForLiveEvent,
} from '@/lib/live-events'
import { useLiveEventsStore } from '@/stores/live-events-store'

/**
 * Keeps build, release and runner queries fresh from `GET /v1/events`.
 * While connected, hooks poll slowly as a fallback; when the stream drops,
 * they return to normal polling until it reconnects.
 */
export function useLiveEvents(enabled: boolean) {
  const { baseUrl, token } = useApiContext()
  const setConnected = useLiveEventsStore((s) => s.setConnected)

  useEffect(() => {
    if (!enabled || !baseUrl || !token) return

    const client = createWebOoreClient({ baseUrl, token })
    const abort = new AbortController()
    const batcher = createTagBatcher((tags) => {
      void queryClient.invalidateQueries({
        predicate: (query) => queryHasTag(query, tags),
      })
    })
    let retryTimer: ReturnType<typeof setTimeout> | undefined
    let attempt = 0
    let hasConnected = false

    async function connect() {
      attempt += 1
      try {
        for await (const event of streamLiveEvents({
          client,
          signal: abort.signal,
        })) {
          if (abort.signal.aborted) return
          if (event.type === 'ready') {
            attempt = 0
            setConnected(true)
            // Events are not replayed, so catch up after a reconnect.
            if (hasConnected) batcher.add(ALL_LIVE_TAGS)
            hasConnected = true
          } else if (event.type === 'resync') {
            batcher.add(ALL_LIVE_TAGS)
          } else if (event.type === 'done') {
            break
          } else {
            batcher.add(tagsForLiveEvent(event))
          }
        }
      } catch {
        // Polling covers the gap until the next attempt.
      }

      if (abort.signal.aborted) return
      setConnected(false)
      retryTimer = setTimeout(() => void connect(), reconnectDelay(attempt))
    }

    void connect()

    return () => {
      abort.abort()
      batcher.cancel()
      if (retryTimer) clearTimeout(retryTimer)
      setConnected(false)
    }
  }, [enabled, baseUrl, token, setConnected])
}
