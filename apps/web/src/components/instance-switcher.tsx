import { useState } from 'react'

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from '@/components/ui/sidebar'
import { ChevronsUpDown, PlusIcon } from 'lucide-react'
import { useActiveInstance, useInstanceStore } from '@/stores/instance-store'
import { getInstanceIcon } from '@/lib/instance-icons'
import { useIsMobile } from '@/hooks/use-mobile'
import { useHotkeys } from '@tanstack/react-hotkeys'
import AddInstanceDialog from '@/components/AddInstanceDialog'

export default function InstanceSwitcher() {
  const [showAddDialog, setShowAddDialog] = useState(false)
  const activeInstance = useActiveInstance()
  const isMobile = useIsMobile()
  const instances = useInstanceStore((state) => state.instances)
  const setActiveInstance = useInstanceStore((state) => state.setActiveInstance)
  const instanceList = Object.values(instances)
  const ActiveInstanceIcon = getInstanceIcon(activeInstance?.icon)

  useHotkeys(
    instanceList.map((instance, idx) => ({
      hotkey: {
        key: (idx + 1).toString(),
        mod: true,
      },
      callback: () => setActiveInstance(instance.id),
    })),
    { preventDefault: true },
  )

  return (
    <>
      <SidebarMenu>
        <SidebarMenuItem>
          <DropdownMenu>
            <DropdownMenuTrigger
              openOnHover
              render={
                <SidebarMenuButton
                  size="lg"
                  className="data-[state=open]:bg-sidebar-accent data-[state=open]:text-sidebar-accent-foreground"
                />
              }
            >
              <div className="flex aspect-square size-8 items-center justify-center rounded-lg bg-sidebar-primary text-sidebar-primary-foreground">
                <ActiveInstanceIcon className="size-4" />
              </div>
              <div className="grid flex-1 text-left text-sm leading-tight">
                <span className="truncate font-medium">
                  {activeInstance?.label}
                </span>
                <span className="truncate text-xs">{activeInstance?.url}</span>
              </div>
              <ChevronsUpDown className="ml-auto" />
            </DropdownMenuTrigger>
            <DropdownMenuContent
              className="w-(--anchor-width) min-w-56"
              align="start"
              side={isMobile ? 'bottom' : 'right'}
              sideOffset={4}
            >
              <DropdownMenuGroup>
                <DropdownMenuLabel className="text-xs text-muted-foreground">
                  Instances
                </DropdownMenuLabel>
                {instanceList.map((candidate, index) => {
                  const CandidateIcon = getInstanceIcon(candidate.icon)

                  return (
                    <DropdownMenuItem
                      key={candidate.id}
                      onClick={() => setActiveInstance(candidate.id)}
                      className="min-w-0 gap-2 rounded-r-none p-2"
                      aria-selected={candidate.id == activeInstance?.id}
                    >
                      <div className="flex size-6 items-center justify-center rounded-md border">
                        <CandidateIcon />
                      </div>
                      {candidate.label}
                      <DropdownMenuShortcut>⌘{index + 1}</DropdownMenuShortcut>
                    </DropdownMenuItem>
                  )
                })}
              </DropdownMenuGroup>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                className="gap-2 p-2"
                onClick={() => setShowAddDialog(true)}
              >
                <div className="flex size-6 items-center justify-center rounded-md border bg-transparent">
                  <PlusIcon className="size-4" />
                </div>
                <div className="font-medium text-muted-foreground">
                  Add instance
                </div>
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </SidebarMenuItem>
      </SidebarMenu>
      <AddInstanceDialog open={showAddDialog} onOpenChange={setShowAddDialog} />
    </>
  )
}
