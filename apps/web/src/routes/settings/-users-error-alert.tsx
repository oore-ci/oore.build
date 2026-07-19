import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import { Info as InformationCircleIcon } from 'lucide-react'

import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'

export function UsersErrorAlert({
  error,
  onRetry,
}: {
  error: unknown
  onRetry: () => void
}) {
  return (
    <Alert variant="destructive">
      <DynamicLucideIcon icon={InformationCircleIcon} size={16} />
      <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <span>
          Failed to load users:{' '}
          {error instanceof Error ? error.message : 'Unknown error'}
        </span>
        <Button variant="outline" size="sm" onClick={onRetry}>
          Retry
        </Button>
      </AlertDescription>
    </Alert>
  )
}
