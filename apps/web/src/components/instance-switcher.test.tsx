import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import InstanceSwitcher from '@/components/InstanceSwitcher'
import { SidebarProvider } from '@/components/ui/sidebar'
import { useInstanceStore } from '@/stores/instance-store'

vi.stubGlobal(
  'matchMedia',
  vi.fn().mockReturnValue({
    matches: false,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  }),
)

describe('InstanceSwitcher', () => {
  beforeEach(() => {
    useInstanceStore.setState({
      activeInstanceId: 'local',
      instances: {
        local: {
          id: 'local',
          label: 'Local Oore',
          url: 'http://127.0.0.1:8787',
          addedAt: 1,
        },
      },
    })
  })

  afterEach(() => {
    useInstanceStore.setState({ activeInstanceId: null, instances: {} })
  })

  it('preloads without replacing focus and opens the full menu on click', async () => {
    render(
      <SidebarProvider>
        <InstanceSwitcher />
      </SidebarProvider>,
    )

    const trigger = screen.getByRole('button', { name: /Local Oore/i })
    trigger.focus()

    await waitFor(() => expect(document.activeElement).toBe(trigger))

    fireEvent.click(trigger)

    expect(await screen.findByRole('menu')).toBeTruthy()
    expect(screen.getByText('Add instance')).toBeTruthy()
  })
})
