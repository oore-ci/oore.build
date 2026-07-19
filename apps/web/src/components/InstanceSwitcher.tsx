import { Suspense, lazy, useState } from 'react'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import { ArrowUpDown as ArrowUpDownIcon } from 'lucide-react'

import { getInstanceIcon } from '@/lib/instance-icons'
import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from '@/components/ui/sidebar'
import { useActiveInstance, useInstanceStore } from '@/stores/instance-store'

const loadInstanceSwitcherMenu = () =>
  import('@/components/instance-switcher-menu')
const InstanceSwitcherMenu = lazy(loadInstanceSwitcherMenu)

function InstanceButton({
  onClick,
  onFocus,
  onMouseEnter,
}: {
  onClick: () => void
  onFocus: () => void
  onMouseEnter: () => void
}) {
  const instance = useActiveInstance()

  const hostname = instance
    ? (() => {
        try {
          return new URL(instance.url).hostname
        } catch {
          return instance.url || 'local'
        }
      })()
    : ''

  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <SidebarMenuButton
          size="lg"
          className="data-open:bg-sidebar-accent data-open:text-sidebar-accent-foreground"
          aria-haspopup="menu"
          onClick={onClick}
          onFocus={onFocus}
          onMouseEnter={onMouseEnter}
        >
          <div className="bg-sidebar-primary text-sidebar-primary-foreground flex aspect-square size-8 items-center justify-center">
            <DynamicLucideIcon
              icon={getInstanceIcon(instance?.icon)}
              size={16}
            />
          </div>
          <div className="grid flex-1 text-left text-sm leading-tight">
            <span className="truncate font-medium">
              {instance?.label ?? 'No instance'}
            </span>
            <span className="truncate text-xs text-muted-foreground">
              {hostname}
            </span>
          </div>
          <DynamicLucideIcon
            icon={ArrowUpDownIcon}
            className="ml-auto"
            size={16}
          />
        </SidebarMenuButton>
      </SidebarMenuItem>
    </SidebarMenu>
  )
}

export default function InstanceSwitcher() {
  const instance = useActiveInstance()
  const instances = useInstanceStore((state) => state.instances)
  const [menuRequested, setMenuRequested] = useState(false)
  const [menuOpen, setMenuOpen] = useState(false)

  if (!instance && Object.keys(instances).length === 0) return null

  function openMenu() {
    setMenuOpen(true)
    setMenuRequested(true)
  }

  const button = (
    <InstanceButton
      onClick={openMenu}
      onFocus={() => void loadInstanceSwitcherMenu()}
      onMouseEnter={() => void loadInstanceSwitcherMenu()}
    />
  )

  if (!menuRequested) return button

  return (
    <Suspense fallback={button}>
      <InstanceSwitcherMenu open={menuOpen} onOpenChange={setMenuOpen} />
    </Suspense>
  )
}
