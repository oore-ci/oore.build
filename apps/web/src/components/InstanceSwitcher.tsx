import { lazy, useState } from 'react'
import { useActiveInstance, useInstanceStore } from '@/stores/instance-store'

const loadInstanceSwitcherMenu = () =>
  import('@/components/instance-switcher-menu')
const InstanceSwitcherMenu = lazy(loadInstanceSwitcherMenu)

export default function InstanceSwitcher() {
  const instance = useActiveInstance()
  const instances = useInstanceStore((state) => state.instances)
  const [menuOpen, setMenuOpen] = useState(false)

  if (!instance && Object.keys(instances).length === 0) return null

  return <InstanceSwitcherMenu open={menuOpen} onOpenChange={setMenuOpen} />
}
