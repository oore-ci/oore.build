import { act, renderHook } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { useDebouncedCallback } from './use-debounced-callback'

afterEach(() => {
  vi.useRealTimers()
})

describe('useDebouncedCallback', () => {
  it('coalesces rapid calls and invokes the latest value after the delay', () => {
    vi.useFakeTimers()
    const callback = vi.fn()
    const { result } = renderHook(() => useDebouncedCallback(callback, 300))

    act(() => {
      result.current('first')
      result.current('second')
      vi.advanceTimersByTime(299)
    })
    expect(callback).not.toHaveBeenCalled()

    act(() => vi.advanceTimersByTime(1))
    expect(callback).toHaveBeenCalledOnce()
    expect(callback).toHaveBeenCalledWith('second')
  })
})
