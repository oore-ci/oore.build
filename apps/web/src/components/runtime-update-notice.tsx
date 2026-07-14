import { Suspense, lazy, useState } from 'react'
import { WorkUpdateIcon } from '@hugeicons/core-free-icons'
import { HugeiconsIcon } from '@hugeicons/react'
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
            <HugeiconsIcon icon={WorkUpdateIcon} size={18} />
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
