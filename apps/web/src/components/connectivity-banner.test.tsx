import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'

import ConnectivityBanner from './connectivity-banner'

describe('ConnectivityBanner', () => {
  beforeEach(() => {
    Object.defineProperty(navigator, 'onLine', {
      configurable: true,
      value: true,
    })
  })

  it('only appears while the browser reports that it is offline', () => {
    render(<ConnectivityBanner />)

    expect(screen.queryByText('You are offline')).toBeNull()

    fireEvent(window, new Event('offline'))
    expect(screen.getByText('You are offline')).toBeTruthy()

    fireEvent(window, new Event('online'))
    expect(screen.queryByText('You are offline')).toBeNull()
  })
})
