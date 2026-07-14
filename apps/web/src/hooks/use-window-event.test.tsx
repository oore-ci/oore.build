import { act, render } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { useWindowEvent } from '@/hooks/use-window-event'

function WindowEventListener({
  value,
  onEvent,
}: {
  value: string
  onEvent: (value: string) => void
}) {
  useWindowEvent('keydown', () => onEvent(value))
  return <output data-testid="value">{value}</output>
}

describe('useWindowEvent', () => {
  it('uses the latest handler and removes the listener on unmount', () => {
    const onEvent = vi.fn()
    const { rerender, unmount } = render(
      <WindowEventListener value="first" onEvent={onEvent} />,
    )

    rerender(<WindowEventListener value="latest" onEvent={onEvent} />)
    act(() => window.dispatchEvent(new KeyboardEvent('keydown')))
    expect(onEvent).toHaveBeenLastCalledWith('latest')

    const callCount = onEvent.mock.calls.length
    unmount()
    act(() => window.dispatchEvent(new KeyboardEvent('keydown')))
    expect(onEvent).toHaveBeenCalledTimes(callCount)
  })
})
