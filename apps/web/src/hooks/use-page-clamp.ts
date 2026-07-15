import { useEffect, useEffectEvent } from 'react'

export function clampCollectionPage(
  page: number,
  pageSize: number,
  total: number,
) {
  return Math.min(page, Math.max(1, Math.ceil(total / pageSize)))
}

/**
 * Keeps a URL-backed page inside the range learned from an asynchronous list
 * response. Synchronizing server pagination with router state is an external
 * effect; the callback is an Effect Event so navigation itself is never a
 * dependency that can restart the synchronization.
 */
export function usePageClamp(
  page: number,
  pageSize: number,
  total: number | undefined,
  onClamp: (page: number) => void,
) {
  const clamp = useEffectEvent(onClamp)

  useEffect(() => {
    if (total === undefined) return
    const nextPage = clampCollectionPage(page, pageSize, total)
    if (nextPage !== page) clamp(nextPage)
  }, [page, pageSize, total])
}
