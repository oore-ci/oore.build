import { Button } from '@/components/ui/button'
import {
  Alert,
  AlertAction,
  AlertDescription,
  AlertTitle,
} from '@/components/ui/alert'
import { Spinner } from '@/components/ui/spinner'
import type { SourceRepositoryFailure } from '@/hooks/use-source-repositories'

function sourceLabel(failure: SourceRepositoryFailure): string {
  const displayName = failure.display_name?.trim()
  if (displayName) return displayName

  const provider =
    failure.provider === 'github'
      ? 'GitHub'
      : failure.provider === 'gitlab'
        ? 'GitLab'
        : 'Local Git'
  return `${provider} (${failure.host_url})`
}

export function SourceDiscoveryWarning({
  failures,
  isRetrying,
  onRetry,
}: {
  failures: Array<SourceRepositoryFailure>
  isRetrying: boolean
  onRetry: () => void
}) {
  if (failures.length === 0) return null

  return (
    <Alert>
      <AlertTitle>
        {failures.length === 1
          ? 'One source could not be loaded'
          : `${failures.length} sources could not be loaded`}
      </AlertTitle>
      <AlertDescription>
        <ul className="mt-1 space-y-1">
          {failures.map((failure) => (
            <li key={failure.integration_id}>
              <span className="font-medium text-foreground">
                {sourceLabel(failure)}
              </span>
              : {failure.message}
            </li>
          ))}
        </ul>
        <p className="mt-2">Other connected sources remain available.</p>
      </AlertDescription>
      <AlertAction>
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={isRetrying}
          onClick={onRetry}
        >
          {isRetrying ? (
            <>
              <Spinner className="size-4" />
              Retrying...
            </>
          ) : (
            'Retry'
          )}
        </Button>
      </AlertAction>
    </Alert>
  )
}
