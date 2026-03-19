import { useNavigate } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowUpDownIcon,
  Settings02Icon,
  Tick02Icon,
} from '@hugeicons/core-free-icons'
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

export default function InstanceSwitcher() {
  const navigate = useNavigate()
  const { isMobile } = useSidebar()
  const instance = useActiveInstance()
  const instances = useInstanceStore((s) => s.instances)
  const setActiveInstance = useInstanceStore((s) => s.setActiveInstance)

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
            <div className="px-2 py-1.5 text-[10px] font-black uppercase tracking-widest text-muted-foreground">
              Runners & Nodes
            </div>
            {instanceList.map((inst) => (
              <DropdownMenuItem
                key={inst.id}
                onClick={() => setActiveInstance(inst.id)}
                className="group gap-2 p-2"
              >
                <div className="flex size-6 items-center justify-center border bg-background">
                  <HugeiconsIcon
                    icon={getInstanceIcon(inst.icon)}
                    size={14}
                  />
                </div>
                <span className="truncate flex-1 font-medium">{inst.label}</span>
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
                void navigate({ to: '/settings/fleet' })
              }}
            >
              <div className="flex size-6 items-center justify-center border bg-muted/20">
                <HugeiconsIcon icon={Settings02Icon} size={14} />
              </div>
              <span className="font-bold text-xs uppercase tracking-widest">Manage Fleet</span>
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </SidebarMenuItem>
    </SidebarMenu>
  )
}
