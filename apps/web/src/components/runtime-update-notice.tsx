import { Suspense, lazy, useState } from 'react'
import {
  BriefcaseBusiness as WorkUpdateIcon,
} from 'lucide-react'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import { useRuntimeUpdates } from '@/hooks/use-runtime-updates'
import {
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
} from '@/components/ui/sidebar'

const RuntimeUpdateDialog = lazy(() => import('./runtime-update-dialog'))

export default function RuntimeUpdateNotice() {
  const [open, setOpen] = useState(false)
  const updates = useRuntimeUpdates()
  const updateCount =
    Number(updates.frontendRelease.data?.update_available === true) +
    Number(updates.backendRelease.data?.update_available === true)

  if (updateCount === 0) return null

  return (
    <>
      <SidebarMenu>
        <SidebarMenuItem>
          <SidebarMenuButton
            variant="outline"
            tooltip={`${updateCount} update${updateCount === 1 ? '' : 's'} available`}
            onClick={() => setOpen(true)}
          >
            <DynamicLucideIcon icon={WorkUpdateIcon} size={18} />
            <span>Updates available</span>
          </SidebarMenuButton>
          <SidebarMenuBadge>{updateCount}</SidebarMenuBadge>
        </SidebarMenuItem>
      </SidebarMenu>

      {open ? (
        <Suspense fallback={null}>
          <RuntimeUpdateDialog open={open} onOpenChange={setOpen} />
        </Suspense>
      ) : null}
    </>
  )
}
