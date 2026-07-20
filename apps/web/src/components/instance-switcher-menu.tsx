import { Suspense, lazy, useState } from 'react'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import {
  Plus as Add01Icon,
  ArrowUpDown as ArrowUpDownIcon,
  Trash2 as Delete02Icon,
  Pencil as PencilEdit02Icon,
  Check as Tick02Icon,
} from 'lucide-react'

import type { Instance } from '@/lib/types'
import { getInstanceIcon } from '@/lib/instance-icons'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from '@/components/ui/sidebar'
import { useActiveInstance, useInstanceStore } from '@/stores/instance-store'

const AddInstanceDialog = lazy(() => import('@/components/AddInstanceDialog'))
const EditInstanceDialog = lazy(() => import('@/components/EditInstanceDialog'))

export default function InstanceSwitcherMenu({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const { isMobile } = useSidebar()
  const instance = useActiveInstance()
  const instances = useInstanceStore((state) => state.instances)
  const setActiveInstance = useInstanceStore((state) => state.setActiveInstance)
  const removeInstance = useInstanceStore((state) => state.removeInstance)
  const [showAddDialog, setShowAddDialog] = useState(false)
  const [editingInstance, setEditingInstance] = useState<Instance | null>(null)

  const instanceList = Object.values(instances)
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
    <>
      <SidebarMenu>
        <SidebarMenuItem>
          <DropdownMenu open={open} onOpenChange={onOpenChange}>
            <DropdownMenuTrigger
              render={
                <SidebarMenuButton
                  size="lg"
                  className="data-open:bg-sidebar-accent data-open:text-sidebar-accent-foreground"
                />
              }
            >
              <div className="flex aspect-square size-8 items-center justify-center bg-sidebar-primary text-sidebar-primary-foreground">
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
            </DropdownMenuTrigger>
            <DropdownMenuContent
              className="min-w-56"
              align="start"
              side={isMobile ? 'bottom' : 'right'}
              sideOffset={4}
            >
              {instanceList.map((candidate) => (
                <DropdownMenuGroup
                  key={candidate.id}
                  className="grid grid-cols-[minmax(0,1fr)_auto]"
                >
                  <DropdownMenuItem
                    onClick={() => setActiveInstance(candidate.id)}
                    className="min-w-0 gap-2 rounded-r-none p-2"
                  >
                    <div className="flex size-6 items-center justify-center border">
                      <DynamicLucideIcon
                        icon={getInstanceIcon(candidate.icon)}
                        size={14}
                      />
                    </div>
                    <span className="min-w-0 flex-1 truncate">
                      {candidate.label}
                    </span>
                    {candidate.id === instance?.id ? (
                      <DynamicLucideIcon
                        icon={Tick02Icon}
                        size={14}
                        className="text-primary"
                      />
                    ) : null}
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    aria-label={`Edit ${candidate.label}`}
                    title={`Edit ${candidate.label}`}
                    className="min-h-9 min-w-9 justify-center rounded-l-none border-l p-2 text-muted-foreground focus:text-foreground"
                    onClick={() => {
                      requestAnimationFrame(() => setEditingInstance(candidate))
                    }}
                  >
                    <DynamicLucideIcon icon={PencilEdit02Icon} size={14} />
                  </DropdownMenuItem>
                </DropdownMenuGroup>
              ))}
              <DropdownMenuSeparator />
              <DropdownMenuItem
                className="gap-2 p-2"
                onClick={() => {
                  requestAnimationFrame(() => setShowAddDialog(true))
                }}
              >
                <div className="flex size-6 items-center justify-center border">
                  <DynamicLucideIcon icon={Add01Icon} size={14} />
                </div>
                Add instance
              </DropdownMenuItem>
              {instance ? (
                <DropdownMenuItem
                  className="gap-2 p-2 text-destructive focus:text-destructive"
                  onClick={() => removeInstance(instance.id)}
                >
                  <div className="flex size-6 items-center justify-center border">
                    <DynamicLucideIcon icon={Delete02Icon} size={14} />
                  </div>
                  Remove {instance.label}
                </DropdownMenuItem>
              ) : null}
            </DropdownMenuContent>
          </DropdownMenu>
        </SidebarMenuItem>
      </SidebarMenu>

      {showAddDialog ? (
        <Suspense>
          <AddInstanceDialog
            open={showAddDialog}
            onOpenChange={setShowAddDialog}
          />
        </Suspense>
      ) : null}
      {editingInstance !== null ? (
        <Suspense>
          <EditInstanceDialog
            instance={editingInstance}
            open
            onOpenChange={(dialogOpen) => {
              if (!dialogOpen) setEditingInstance(null)
            }}
          />
        </Suspense>
      ) : null}
    </>
  )
}
