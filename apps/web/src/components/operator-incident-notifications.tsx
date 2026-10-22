import { HugeiconsIcon } from '@hugeicons/react'
import { Notification03Icon } from '@hugeicons/core-free-icons'

import { useHasPermission } from '@/hooks/use-permissions'
import {
  useMarkOperatorIncidentRead,
  useOperatorIncidents,
} from '@/hooks/use-operator-incidents'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'

export function OperatorIncidentNotifications() {
  const canManageSources = useHasPermission('integrations:write')
  const incidentsQuery = useOperatorIncidents({ enabled: canManageSources })
  const markRead = useMarkOperatorIncidentRead()
  const incidents = incidentsQuery.data?.incidents ?? []
  const unreadCount = incidents.filter((incident) => !incident.read_at).length

  if (!canManageSources) return null

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            aria-label={`${unreadCount} unread operator notifications`}
            className="relative"
            size="icon"
            variant="ghost"
          />
        }
      >
        <HugeiconsIcon icon={Notification03Icon} />
        {unreadCount > 0 ? (
          <span className="absolute top-1 right-1 size-2 rounded-full bg-destructive" />
        ) : null}
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-80">
        <DropdownMenuGroup>
          <DropdownMenuLabel>Notifications</DropdownMenuLabel>
          {incidents.length === 0 ? (
            <p className="px-2 py-3 text-xs text-muted-foreground">
              No open incidents.
            </p>
          ) : (
            incidents.map((incident) => (
              <DropdownMenuItem
                key={incident.id}
                render={<a href={incident.repair_url} />}
                onClick={() => markRead.mutate(incident.id)}
              >
                <span className="flex min-w-0 flex-col">
                  <span className="truncate font-medium">
                    {incident.resource_name}
                  </span>
                  <span className="text-xs text-muted-foreground">
                    {incident.resource_kind === 'runner'
                      ? 'Low disk space'
                      : `Credential ${incident.reason.replace('_', ' ')}`}
                  </span>
                </span>
              </DropdownMenuItem>
            ))
          )}
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
