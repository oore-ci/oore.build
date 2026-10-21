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
      return `${incident.resource_name} rejected its stored credential. The provider did not prove that it expired.`
    case 'low_disk':
      return `${incident.resource_name} is running out of disk space. Builds on it may fail.`
    default:
      return incident.reason
  }
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
          render={<a href={incident.repair_url} onClick={onRead} />}
          size="sm"
          variant="outline"
        >
          {incident.repair_action}
        </Button>
      </AlertDescription>
    </Alert>
  )
}
