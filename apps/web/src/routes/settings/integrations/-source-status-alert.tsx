import { HugeiconsIcon } from '@hugeicons/react'
import { Alert02Icon } from '@hugeicons/core-free-icons'

import { relativeTime } from '@/lib/format-utils'
import {
  integrationStatusReason,
  type IntegrationWithStatus,
} from '@/lib/integration-status'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'

// Explains why a source is in error when no open incident already does, and
// offers the fix next to the explanation.
export function SourceStatusAlert({
  canWrite,
  checkPending,
  integration,
  onCheckAgain,
  onReconnect,
  reconnectLabel,
}: {
  canWrite: boolean
  checkPending: boolean
  integration: IntegrationWithStatus
  onCheckAgain?: () => void
  onReconnect?: () => void
  reconnectLabel: string
}) {
  const reason = integrationStatusReason(integration)
  if (!reason) return null

  return (
    <Alert variant="destructive">
      <HugeiconsIcon icon={Alert02Icon} />
      <AlertTitle>This source cannot be used</AlertTitle>
      <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <span>
          {reason} Builds from its repositories cannot check out code until it
          is fixed.
          {integration.status_checked_at
            ? ` Last checked ${relativeTime(integration.status_checked_at)}.`
            : ''}
          {canWrite ? '' : ' Ask an owner or admin to reconnect it.'}
        </span>
        {canWrite && (onReconnect || onCheckAgain) ? (
          <span className="flex shrink-0 gap-2">
            {onCheckAgain ? (
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={checkPending}
                onClick={onCheckAgain}
              >
                {checkPending ? 'Checking...' : 'Check again'}
              </Button>
            ) : null}
            {onReconnect ? (
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={onReconnect}
              >
                {reconnectLabel}
              </Button>
            ) : null}
          </span>
        ) : null}
      </AlertDescription>
    </Alert>
  )
}
