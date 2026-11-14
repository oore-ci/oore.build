import { useEffect } from 'react'
import type { BuildStatus } from '@oore/client/models'

type TabState = 'waiting' | 'running' | 'succeeded' | 'failed' | 'stopped'

/** How a build status reads in a background browser tab. */
export function buildTabState(
  status: BuildStatus | undefined,
): TabState | null {
  switch (status) {
    case undefined:
      return null
    case 'queued':
    case 'scheduled':
    case 'assigned':
      return 'waiting'
    case 'running':
      return 'running'
    case 'succeeded':
      return 'succeeded'
    case 'failed':
    case 'timed_out':
      return 'failed'
    case 'canceled':
    case 'expired':
      return 'stopped'
  }
}

/** A short mark before the page title, readable in a narrow tab. */
export const BUILD_TAB_TITLE_MARK = {
  waiting: '… ',
  running: '● ',
  succeeded: '✓ ',
  failed: '✕ ',
  stopped: '',
} satisfies Record<TabState, string>

// Read from the theme so the dot matches the status colours on the page.
const DOT_COLOR_VARIABLE = {
  waiting: null,
  running: '--info',
  succeeded: '--success',
  failed: '--destructive',
  stopped: null,
} satisfies Record<TabState, string | null>

const FAVICON_SIZE = 64

/**
 * Puts a coloured dot on the browser tab's icon while a build page is open,
 * so a build left in a background tab shows when it finishes.
 */
export function useBuildTabIcon(status: BuildStatus | undefined) {
  const state = buildTabState(status)
  useEffect(() => {
    const variable = state ? DOT_COLOR_VARIABLE[state] : null
    const link = document.querySelector<HTMLLinkElement>(
      'link[rel="icon"][type="image/svg+xml"]',
    )
    if (!variable || !link) return
    const color = getComputedStyle(document.documentElement)
      .getPropertyValue(variable)
      .trim()
    if (!color) return

    const original = link.href
    let canceled = false
    const image = new Image()
    image.onload = () => {
      if (canceled) return
      const canvas = document.createElement('canvas')
      canvas.width = FAVICON_SIZE
      canvas.height = FAVICON_SIZE
      const context = canvas.getContext('2d')
      if (!context) return
      context.drawImage(image, 0, 0, FAVICON_SIZE, FAVICON_SIZE)
      const radius = FAVICON_SIZE * 0.2
      const center = FAVICON_SIZE - radius - 2
      context.beginPath()
      context.arc(center, center, radius + 3, 0, Math.PI * 2)
      context.fillStyle = 'white'
      context.fill()
      context.beginPath()
      context.arc(center, center, radius, 0, Math.PI * 2)
      context.fillStyle = color
      context.fill()
      link.href = canvas.toDataURL('image/png')
    }
    image.src = original
    return () => {
      canceled = true
      link.href = original
    }
  }, [state])
}
