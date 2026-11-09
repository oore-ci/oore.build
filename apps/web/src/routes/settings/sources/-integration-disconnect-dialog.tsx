import { HugeiconsIcon } from '@hugeicons/react'
import { InformationCircleIcon } from '@hugeicons/core-free-icons'

import type { Project } from '@oore/client/models'
import type { Integration } from '@oore/client/models'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { isNearScrollEnd } from '@/lib/scroll'
import { Text } from '@/components/layout/text'

export function IntegrationDisconnectDialog({
  affectedProjects,
  affectedProjectTotal,
  error,
  hasMoreProjects,
  integration,
  isLoading,
  isPending,
  isFetchingMoreProjects,
  onConfirm,
  onLoadMoreProjects,
  onOpenChange,
  onRetry,
  open,
  repositoryCount,
}: {
  affectedProjects: Array<Project>
  affectedProjectTotal: number
  error: Error | null
  hasMoreProjects: boolean
  integration: Integration
  isLoading: boolean
  isPending: boolean
  isFetchingMoreProjects: boolean
  onConfirm: () => void
  onLoadMoreProjects: () => void
  onOpenChange: (open: boolean) => void
  onRetry?: () => void
  open: boolean
  repositoryCount: number
}) {
  const sourceName = integration.display_name ?? integration.provider
  const previewReady = !isLoading && !error

  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Disconnect {sourceName}?</AlertDialogTitle>
          <AlertDialogDescription>
            Credentials, installations, {repositoryCount}{' '}
            {repositoryCount === 1 ? 'repository record' : 'repository records'}
            , and source webhook configuration will be removed.
          </AlertDialogDescription>
        </AlertDialogHeader>

        <div className="space-y-3" aria-live="polite">
          <h3 className="text-sm font-semibold">Affected projects</h3>
          {isLoading ? (
            <div className="space-y-2" aria-label="Checking affected projects">
              <Skeleton className="h-4 w-56" />
              <Skeleton className="h-4 w-40" />
            </div>
          ) : error ? (
            <Alert variant="destructive">
              <HugeiconsIcon icon={InformationCircleIcon} aria-hidden />
              <AlertDescription className="space-y-3">
                <p>Could not load the disconnect preview: {error.message}</p>
                {onRetry ? (
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={onRetry}
                  >
                    Retry preview
                  </Button>
                ) : null}
              </AlertDescription>
            </Alert>
          ) : affectedProjectTotal > 0 ? (
            <Alert>
              <AlertTitle>
                {affectedProjectTotal}{' '}
                {affectedProjectTotal === 1 ? 'project is' : 'projects are'}{' '}
                currently linked to this source.
              </AlertTitle>
              <AlertDescription>
                <ul
                  className="max-h-40 list-disc overflow-y-auto pl-4"
                  onScroll={(event) => {
                    const list = event.currentTarget
                    if (
                      isNearScrollEnd(list) &&
                      hasMoreProjects &&
                      !isFetchingMoreProjects
                    ) {
                      onLoadMoreProjects()
                    }
                  }}
                >
                  {affectedProjects.map((project) => (
                    <li key={project.id} className="py-1 font-medium">
                      {project.name}
                    </li>
                  ))}
                  {isFetchingMoreProjects ? (
                    <li className="list-none py-1 text-muted-foreground">
                      Loading more projects…
                    </li>
                  ) : null}
                </ul>
              </AlertDescription>
            </Alert>
          ) : (
            <Text variant="muted">
              No current projects are linked to repositories from this source.
            </Text>
          )}

          {previewReady ? (
            <Alert>
              <AlertDescription>
                Projects remain, but their repository links are cleared and new
                builds cannot use this source until it is reconnected. This
                preview uses current project links; changes made after this
                check may not appear.
              </AlertDescription>
            </Alert>
          ) : null}
        </div>

        <AlertDialogFooter>
          <AlertDialogCancel disabled={isPending}>Cancel</AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            onClick={onConfirm}
            disabled={!previewReady || isPending}
          >
            {isPending ? 'Disconnecting...' : 'Disconnect source'}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
