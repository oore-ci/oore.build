import { useEffect } from 'react'
import type { Virtualizer } from '@tanstack/react-virtual'

export function useAutoScroll(
  virtualizer: Virtualizer<HTMLDivElement, Element>,
  count: number,
  enabled: boolean,
) {
  useEffect(() => {
    if (enabled && count > 0) virtualizer.scrollToIndex(count - 1, { align: 'end' })
  }, [count, enabled, virtualizer])
}
