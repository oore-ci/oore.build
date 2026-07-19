import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import { Info as InformationCircleIcon } from 'lucide-react'

import type { Integration, Project } from '@/lib/types'
import { Alert, AlertDescription } from '@/components/ui/alert'
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

export function IntegrationDisconnectDialog({
  affectedProjects,
  error,
  integration,
  isLoading,
  isPending,
  onConfirm,
  onOpenChange,
  onRetry,
  open,
  repositoryCount,
}: {
  affectedProjects: Array<Project>
  error: Error | null
  integration: Integration
  isLoading: boolean
  isPending: boolean
  onConfirm: () => void
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
              <DynamicLucideIcon icon={InformationCircleIcon} aria-hidden />
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
          ) : affectedProjects.length > 0 ? (
            <div className="border bg-muted/40">
              <p className="border-b px-3 py-2 text-sm">
                {affectedProjects.length}{' '}
                {affectedProjects.length === 1 ? 'project is' : 'projects are'}{' '}
                currently linked to this source.
              </p>
              <ul className="max-h-40 overflow-y-auto px-3 py-2 text-sm">
                {affectedProjects.map((project) => (
                  <li key={project.id} className="py-1 font-medium">
                    {project.name}
                  </li>
                ))}
              </ul>
            </div>
          ) : (
            <p className="text-sm text-muted-foreground">
              No current projects are linked to repositories from this source.
            </p>
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
