import type { Query } from '@tanstack/react-query'
import type { LiveEvent } from '@oore/client/events'

/** Poll interval for live data while the event stream is connected. */
export const LIVE_FALLBACK_POLL_MS = 30_000

const MAX_RECONNECT_DELAY_MS = 30_000

/** OpenAPI tags whose cached queries a live event makes stale. */
export function tagsForLiveEvent(event: LiveEvent): Array<string> {
  switch (event.type) {
    case 'build':
      return ['Builds', 'Apps', 'Artifacts', INBOX_TAG]
    case 'release':
      return ['Releases', INBOX_TAG]
    case 'runner':
      return ['Runners']
    default:
      return []
  }
}

/**
 * The personal inbox has the Users tag, which also covers queries that no
 * build or release changes, so it is matched by operation instead.
 */
const INBOX_TAG = 'Inbox'
const INBOX_OPERATION = 'getInbox'

export const ALL_LIVE_TAGS = [
  INBOX_TAG,
  'Builds',
  'Apps',
  'Artifacts',
  'Releases',
  'Runners',
]

/** Whether a scoped generated query (`[instanceId, { tags }]`) has any tag. */
export function queryHasTag(query: Pick<Query, 'queryKey'>, tags: Set<string>) {
  // SAFETY: generated keys are `[instanceId, { _id, baseUrl, tags? }]`;
  // other keys have no object at index 1, so `tags` reads as undefined.
  const params = query.queryKey[1] as
    | { _id?: string; tags?: ReadonlyArray<string> }
    | null
    | undefined
  if (tags.has(INBOX_TAG) && params?._id === INBOX_OPERATION) return true
  return params?.tags?.some((tag) => tags.has(tag)) ?? false
}

/** Exponential reconnect delay: 1s, 2s, 4s ... capped at 30s. */
export function reconnectDelay(attempt: number) {
  return Math.min(1_000 * 2 ** Math.max(0, attempt - 1), MAX_RECONNECT_DELAY_MS)
}

/** Coalesces tags from bursts of events into one invalidation. */
export function createTagBatcher(
  onFlush: (tags: Set<string>) => void,
  delayMs = 250,
) {
  let timer: ReturnType<typeof setTimeout> | undefined
  let pending = new Set<string>()

  return {
    add(tags: Iterable<string>) {
      for (const tag of tags) pending.add(tag)
      if (timer || pending.size === 0) return
      timer = setTimeout(() => {
        timer = undefined
        const flushed = pending
        pending = new Set()
        onFlush(flushed)
      }, delayMs)
    },
    cancel() {
      if (timer) clearTimeout(timer)
      timer = undefined
      pending = new Set()
    },
  }
}
