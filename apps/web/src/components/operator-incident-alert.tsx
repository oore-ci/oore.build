import { Link } from '@tanstack/react-router'

import type { OperatorIncident } from '@oore/client/models'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'

function incidentDescription(incident: OperatorIncident): string {
  switch (incident.reason) {
    case 'expired':
      return `The stored credential for ${incident.resource_name} expired. Builds cannot use this source.`
    case 'expiring':
      return `The stored credential for ${incident.resource_name} expires soon.`
    case 'refresh_failed':
      return `Oore could not refresh the credential for ${incident.resource_name}. Builds cannot use this source.`
    case 'rejected':
      return `${incident.resource_name} no longer accepts the stored credential. Builds cannot use this source until you reconnect it.`
    case 'low_disk':
      return `${incident.resource_name} is running out of disk space. Builds on it may fail.`
    default:
      return incident.reason
  }
}

/** One short line naming what broke, for lists such as Needs attention and the bell. */
export function incidentTitle(incident: OperatorIncident): string {
  switch (incident.reason) {
    case 'expired':
      return `${incident.resource_name} credential expired`
    case 'expiring':
      return `${incident.resource_name} credential expires soon`
    case 'refresh_failed':
      return `${incident.resource_name} credential could not be refreshed`
    case 'rejected':
      return `${incident.resource_name} rejected its credential`
    case 'low_disk':
      return `${incident.resource_name} is low on disk space`
    default:
      return `${incident.resource_name} needs attention`
  }
}

/**
 * Router props for an incident's repair page, so following it keeps the app
 * loaded and the mark-as-read request alive.
 */
export function incidentLinkProps(incident: OperatorIncident) {
  const url = new URL(incident.repair_url, window.location.origin)
  // SAFETY: the daemon only issues repair URLs for routes this app defines,
  // with search params those routes accept.
  const to = url.pathname as '/'
  // SAFETY: as above; the search params come from the same repair URL.
  const search = Object.fromEntries(url.searchParams) as never
  return { to, search }
}

export function OperatorIncidentAlert({
  incident,
  onRead,
}: {
  incident: OperatorIncident
  onRead?: () => void
}) {
  return (
    <Alert
      variant={incident.severity === 'critical' ? 'destructive' : 'default'}
    >
      <AlertTitle>
        {incident.resource_kind === 'runner'
          ? 'Runner needs attention'
          : 'Source credential needs attention'}
      </AlertTitle>
      <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <span>
          {incidentDescription(incident)} Occurred {incident.occurrence_count}{' '}
          {incident.occurrence_count === 1 ? 'time' : 'times'}.
        </span>
        <Button
          nativeButton={false}
          render={<Link {...incidentLinkProps(incident)} onClick={onRead} />}
          size="sm"
          variant="outline"
        >
          {incident.repair_action}
        </Button>
      </AlertDescription>
    </Alert>
  )
}
