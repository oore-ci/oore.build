import { Suspense, lazy, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  ArrowUpDownIcon,
  Delete02Icon,
  PencilEdit02Icon,
  Tick02Icon,
} from '@hugeicons/core-free-icons'
import type { Instance } from '@/lib/types'
import { getInstanceIcon } from '@/lib/instance-icons'
import {
  DropdownMenu,
  DropdownMenuContent,
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

export default function InstanceSwitcher() {
  const { isMobile } = useSidebar()
  const instance = useActiveInstance()
  const instances = useInstanceStore((s) => s.instances)
  const setActiveInstance = useInstanceStore((s) => s.setActiveInstance)
  const removeInstance = useInstanceStore((s) => s.removeInstance)
  const [showAddDialog, setShowAddDialog] = useState(false)
  const [editingInstance, setEditingInstance] = useState<Instance | null>(null)

  const instanceList = Object.values(instances)

  if (!instance && instanceList.length === 0) return null

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
          <DropdownMenu>
            <DropdownMenuTrigger
              render={
                <SidebarMenuButton
                  size="lg"
                  className="data-open:bg-sidebar-accent data-open:text-sidebar-accent-foreground"
                />
              }
            >
              <div className="bg-sidebar-primary text-sidebar-primary-foreground flex aspect-square size-8 items-center justify-center">
                <HugeiconsIcon
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
              <HugeiconsIcon
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
              {instanceList.map((inst) => (
                <DropdownMenuItem
                  key={inst.id}
                  onClick={() => setActiveInstance(inst.id)}
                  className="group gap-2 p-2"
                >
                  <div className="flex size-6 items-center justify-center border">
                    <HugeiconsIcon
                      icon={getInstanceIcon(inst.icon)}
                      size={14}
                    />
                  </div>
                  <span className="truncate flex-1">{inst.label}</span>
                  <button
                    type="button"
                    className="ml-auto text-muted-foreground opacity-0 transition-opacity hover:text-foreground group-hover:opacity-100 focus-visible:opacity-100"
                    onClick={(e) => {
                      e.stopPropagation()
                      requestAnimationFrame(() => setEditingInstance(inst))
                    }}
                    title={`Edit ${inst.label}`}
                    aria-label={`Edit ${inst.label}`}
                  >
                    <HugeiconsIcon icon={PencilEdit02Icon} size={14} />
                  </button>
                  {inst.id === instance?.id ? (
                    <HugeiconsIcon
                      icon={Tick02Icon}
                      size={14}
                      className="text-primary"
                    />
                  ) : null}
                </DropdownMenuItem>
              ))}
              <DropdownMenuSeparator />
              <DropdownMenuItem
                className="gap-2 p-2"
                onClick={() => {
                  requestAnimationFrame(() => setShowAddDialog(true))
                }}
              >
                <div className="flex size-6 items-center justify-center border bg-background">
                  <HugeiconsIcon icon={Add01Icon} size={14} />
                </div>
                Add instance
              </DropdownMenuItem>
              {instance ? (
                <DropdownMenuItem
                  className="gap-2 p-2 text-destructive focus:text-destructive"
                  onClick={() => removeInstance(instance.id)}
                >
                  <div className="flex size-6 items-center justify-center border">
                    <HugeiconsIcon icon={Delete02Icon} size={14} />
                  </div>
                  Remove {instance.label}
                </DropdownMenuItem>
              ) : null}
            </DropdownMenuContent>
          </DropdownMenu>
        </SidebarMenuItem>
      </SidebarMenu>

      {showAddDialog && (
        <Suspense>
          <AddInstanceDialog
            open={showAddDialog}
            onOpenChange={setShowAddDialog}
          />
        </Suspense>
      )}
      {editingInstance !== null && (
        <Suspense>
          <EditInstanceDialog
            instance={editingInstance}
            open
            onOpenChange={(open) => {
              if (!open) setEditingInstance(null)
            }}
          />
        </Suspense>
      )}
    </>
  )
}
