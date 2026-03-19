import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useMemo, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  Delete02Icon,
  PencilEdit01Icon,
  Tick01Icon,
  UserIcon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { PageMeta } from '@/lib/seo'
import { useInstanceStore } from '@/stores/instance-store'
import { INSTANCE_ICONS } from '@/lib/instance-icons'
import AddInstanceDialog from '@/components/AddInstanceDialog'
import { useHasPermission } from '@/hooks/use-permissions'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'

export const Route = createFileRoute('/settings/fleet')({
  staticData: { breadcrumbLabel: 'Nodes' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: FleetRegistryPage,
})

function getAuthUserFromStorage(instanceId: string) {
  try {
    const val = localStorage.getItem(`oore_auth_user_${instanceId}`)
    if (!val) return null
    return JSON.parse(val) as { email: string; role: string }
  } catch {
    return null
  }
}

function FleetRegistryPage() {
  const navigate = useNavigate()
  const instances = useInstanceStore((s) => s.instances)
  const activeId = useInstanceStore((s) => s.activeInstanceId)
  const setActiveInstance = useInstanceStore((s) => s.setActiveInstance)
  const removeInstance = useInstanceStore((s) => s.removeInstance)
  const [showAddDialog, setShowAddDialog] = useState(false)
  const [editInstanceId, setEditInstanceId] = useState<string | undefined>()

  const canWrite = useHasPermission('fleet', 'write')

  const sortedInstances = useMemo(() => {
    return Object.values(instances).sort(
      (a, b) => (b.addedAt ?? 0) - (a.addedAt ?? 0),
    )
  }, [instances])

  return (
    <PageLayout width="wide">
      <PageMeta title="Fleet Registry" noindex />
      <PageHeader
        title="Fleet Registry"
        description="Manage your connected Oore CI instances and active sessions."
        actions={
          canWrite && (
            <Button onClick={() => setShowAddDialog(true)}>
              <HugeiconsIcon icon={Add01Icon} size={16} />
              Add Instance
            </Button>
          )
        }
      />

      <div className="rounded-none border-2 border-border/40 bg-muted/5">
        <div className="pb-3 border-b border-border/40 p-6">
          <p className="text-[10px] font-black uppercase tracking-[0.3em] text-muted-foreground">
            Instance Fleet Inventory
          </p>
        </div>
        <div className="p-0">
          <Table>
            <TableHeader className="bg-muted/30">
              <TableRow className="hover:bg-transparent border-border/40">
                <TableHead className="w-[80px] text-[10px] font-black uppercase tracking-widest pl-6">
                  Icon
                </TableHead>
                <TableHead className="text-[10px] font-black uppercase tracking-widest">
                  Instance Node
                </TableHead>
                <TableHead className="text-[10px] font-black uppercase tracking-widest">
                  Session
                </TableHead>
                <TableHead className="text-[10px] font-black uppercase tracking-widest">
                  Status
                </TableHead>
                <TableHead className="text-right text-[10px] font-black uppercase tracking-widest pr-6">
                  Command
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {sortedInstances.map((inst) => {
                const isActive = inst.id === activeId
                const auth = getAuthUserFromStorage(inst.id)
                const iconEntry =
                  INSTANCE_ICONS.find((i) => i.key === inst.icon) ??
                  INSTANCE_ICONS[0]

                return (
                  <TableRow
                    key={inst.id}
                    className="group border-border/40 hover:bg-primary/5 h-20"
                  >
                    <TableCell className="pl-6">
                      <div
                        className={`flex size-14 items-center justify-center border-2 bg-background text-primary shadow-inner rounded-none ${isActive ? `border-primary/50` : `border-border`}`}
                      >
                        <HugeiconsIcon icon={iconEntry.icon} size={28} />
                      </div>
                    </TableCell>
                    <TableCell>
                      <div className="space-y-1">
                        <p className="font-black uppercase tracking-tight text-sm line-clamp-1">
                          {inst.label}
                        </p>
                        <p className="font-mono text-[9px] uppercase tracking-widest opacity-60 truncate max-w-[200px]">
                          {inst.url || 'local dev proxy'}
                        </p>
                      </div>
                    </TableCell>
                    <TableCell>
                      {auth ? (
                        <div className="space-y-1">
                          <div className="flex items-center gap-2 text-xs font-bold truncate max-w-[150px]">
                            <HugeiconsIcon
                              icon={UserIcon}
                              size={12}
                              className="text-primary"
                            />
                            {auth.email}
                          </div>
                          <p className="text-[10px] font-black uppercase tracking-widest text-muted-foreground opacity-50">
                            {auth.role}
                          </p>
                        </div>
                      ) : (
                        <p className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground/40 italic">
                          Inert
                        </p>
                      )}
                    </TableCell>
                    <TableCell>
                      {isActive ? (
                        <Badge
                          variant="success"
                          className="rounded-none px-2 py-1 text-[8px] font-black uppercase tracking-widest"
                        >
                          Active Node
                        </Badge>
                      ) : (
                        <Badge
                          variant="outline"
                          className="rounded-none px-2 py-1 text-[8px] font-black uppercase tracking-widest opacity-40"
                        >
                          Standby
                        </Badge>
                      )}
                    </TableCell>
                    <TableCell className="text-right pr-6">
                      <div className="flex items-center justify-end gap-2">
                        {!isActive ? (
                          <Button
                            variant="outline"
                            size="sm"
                            className="rounded-none border-2 border-primary/20 bg-primary/5 font-black uppercase tracking-widest text-[9px] h-8 hover:bg-primary/10 hover:border-primary/40"
                            onClick={() => {
                              setActiveInstance(inst.id)
                              toast.success(`Switched to ${inst.label}`)
                              void navigate({ to: '/' })
                            }}
                          >
                            <HugeiconsIcon icon={Tick01Icon} size={12} />
                            Deploy
                          </Button>
                        ) : (
                          <div className="flex items-center gap-1.5 px-3 py-1.5 border border-primary/40 bg-primary/5">
                            <span className="size-1.5 rounded-full bg-primary animate-pulse" />
                            <span className="text-[9px] font-black uppercase tracking-[0.2em] text-primary">
                              Engaged
                            </span>
                          </div>
                        )}

                        {canWrite && (
                          <Button
                            variant="ghost"
                            size="icon"
                            className="size-8 rounded-none border border-border/40 text-muted-foreground hover:bg-primary/10 hover:text-primary hover:border-primary/40"
                            onClick={() => {
                              setEditInstanceId(inst.id)
                              setShowAddDialog(true)
                            }}
                          >
                            <HugeiconsIcon icon={PencilEdit01Icon} size={16} />
                          </Button>
                        )}

                        {canWrite && !isActive && (
                          <Button
                            variant="ghost"
                            size="icon"
                            className="size-8 rounded-none border border-border/40 text-muted-foreground hover:bg-destructive/10 hover:text-destructive hover:border-destructive/40"
                            onClick={() => {
                              removeInstance(inst.id)
                              toast.success('Node decommissioned from fleet')
                            }}
                          >
                            <HugeiconsIcon icon={Delete02Icon} size={16} />
                          </Button>
                        )}
                      </div>
                    </TableCell>
                  </TableRow>
                )
              })}
            </TableBody>
          </Table>

          {canWrite && (
            <div className="p-4 border-t border-border/40 bg-background/50">
              <button
                onClick={() => setShowAddDialog(true)}
                className="flex w-full items-center justify-center gap-3 border-2 border-dashed border-border/40 py-6 transition-all duration-300 hover:border-primary/50 hover:bg-primary/5 group"
              >
                <HugeiconsIcon
                  icon={Add01Icon}
                  size={20}
                  className="text-muted-foreground group-hover:text-primary"
                />
                <span className="text-[10px] font-black uppercase tracking-[0.3em] text-muted-foreground group-hover:text-primary">
                  Commission New Instance Node
                </span>
              </button>
            </div>
          )}
        </div>
      </div>

      <AddInstanceDialog
        open={showAddDialog}
        onOpenChange={(open) => {
          setShowAddDialog(open)
          if (!open) setEditInstanceId(undefined)
        }}
        editInstanceId={editInstanceId}
      />
    </PageLayout>
  )
}
