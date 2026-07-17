import { beforeEach, describe, expect, it } from 'vitest'

import { useUiStore } from './ui-store'

beforeEach(() => {
  localStorage.clear()
  useUiStore.setState({
    commandPaletteOpen: false,
    directRunnerTrustNoticeAcknowledgements: {},
    sidebarOpen: true,
  })
})

describe('useUiStore', () => {
  it('persists sidebar and direct runner trust acknowledgements', () => {
    useUiStore.getState().setSidebarOpen(false)
    useUiStore.getState().setCommandPaletteOpen(true)
    useUiStore
      .getState()
      .acknowledgeDirectRunnerTrustNotice(
        'direct-runner-protocol-4:instance-1:user-1',
      )

    const stored = JSON.parse(
      localStorage.getItem('oore-ui-preferences') ?? '{}',
    ) as { state?: Record<string, unknown> }

    expect(stored.state).toEqual({
      directRunnerTrustNoticeAcknowledgements: {
        'direct-runner-protocol-4:instance-1:user-1': true,
      },
      sidebarOpen: false,
    })
  })

  it('restores the sidebar preference from a previous session', async () => {
    localStorage.setItem(
      'oore-ui-preferences',
      JSON.stringify({ state: { sidebarOpen: false }, version: 0 }),
    )

    await useUiStore.persist.rehydrate()

    expect(useUiStore.getState().sidebarOpen).toBe(false)
    expect(
      useUiStore.getState().directRunnerTrustNoticeAcknowledgements,
    ).toEqual({})
  })

  it('restores direct runner trust acknowledgements', async () => {
    localStorage.setItem(
      'oore-ui-preferences',
      JSON.stringify({
        state: {
          directRunnerTrustNoticeAcknowledgements: {
            'direct-runner-protocol-4:instance-1:user-1': true,
          },
          sidebarOpen: true,
        },
        version: 0,
      }),
    )

    await useUiStore.persist.rehydrate()

    expect(
      useUiStore.getState().directRunnerTrustNoticeAcknowledgements,
    ).toEqual({
      'direct-runner-protocol-4:instance-1:user-1': true,
    })
  })
})
