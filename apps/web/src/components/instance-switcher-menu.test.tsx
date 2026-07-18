import { render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import InstanceSwitcherMenu from './instance-switcher-menu'
import { SidebarProvider } from './ui/sidebar'
import { useInstanceStore } from '@/stores/instance-store'

vi.stubGlobal(
  'matchMedia',
  vi.fn().mockReturnValue({
    matches: false,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  }),
)

describe('InstanceSwitcherMenu', () => {
  afterEach(() => {
    useInstanceStore.setState({ instances: {}, activeInstanceId: null })
  })

  it('exposes edit as its own keyboard-addressable menu item', () => {
    useInstanceStore.setState({
      instances: {
        'instance-1': {
          id: 'instance-1',
          label: 'Build Mac',
          url: 'https://ci.example.com',
          addedAt: 1,
        },
      },
      activeInstanceId: 'instance-1',
    })

    render(
      <SidebarProvider>
        <InstanceSwitcherMenu open onOpenChange={vi.fn()} />
      </SidebarProvider>,
    )

    const editItem = screen.getByRole('menuitem', { name: 'Edit Build Mac' })
    expect(editItem).toBeTruthy()
    expect(editItem.querySelector('button')).toBeNull()
  })
})
