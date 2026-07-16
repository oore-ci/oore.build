import {
  ArrowRight01Icon,
  ArrowUpRight01Icon,
  Download04Icon,
} from '@hugeicons/core-free-icons'
import { HugeiconsIcon } from '@hugeicons/react'
import { toast } from 'sonner'
import type { RuntimeReleaseStatus } from '@/lib/types'
import { useRuntimeUpdates } from '@/hooks/use-runtime-updates'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Spinner } from '@/components/ui/spinner'
import { formatReleaseNotes } from '@/components/runtime-update-utils'

function updateButtonLabel(phase: string | undefined, pending: boolean) {
  if (pending) return 'Starting...'
  if (phase === 'restarting') return 'Restarting...'
  if (phase === 'updating') return 'Updating...'
  return 'Update now'
}

function RuntimeUpdateCard({
  name,
  release,
  phase,
  pending,
  managed,
  onUpdate,
}: {
  name: string
  release: RuntimeReleaseStatus
  phase?: string
  pending: boolean
  managed: boolean
  onUpdate: () => void
}) {
  const busy = phase === 'updating' || phase === 'restarting' || pending

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
          {name}
        </CardTitle>
        <CardDescription>{release.channel} channel</CardDescription>
        <CardAction>
          <Badge variant="outline">Update available</Badge>
        </CardAction>
      </CardHeader>
      <CardContent>
        <div className="flex items-center gap-2">
          <span className="font-mono text-xs text-muted-foreground">
            {release.version}
          </span>
          <HugeiconsIcon
            icon={ArrowRight01Icon}
            size={14}
            className="shrink-0 text-muted-foreground"
          />
          <span className="font-mono text-xs font-medium">
            {release.latest_version}
          </span>
        </div>
        {!managed ? (
          <p className="mt-2 text-xs text-muted-foreground">
            Install this runtime as a managed service to update it here.
          </p>
        ) : null}
      </CardContent>
      <CardFooter>
        <Button
          size="sm"
          className="w-full"
          disabled={!managed || busy}
          onClick={onUpdate}
        >
          {busy ? (
            <Spinner />
          ) : (
            <HugeiconsIcon icon={Download04Icon} data-icon="inline-start" />
          )}
          {updateButtonLabel(phase, pending)}
        </Button>
      </CardFooter>
    </Card>
  )
}

export default function RuntimeUpdateDialog({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const updates = useRuntimeUpdates()
  const frontend = updates.frontendRelease.data
  const backend = updates.backendRelease.data
  const frontendAvailable = frontend?.update_available === true
  const backendAvailable = backend?.update_available === true
  const releases = [
    frontendAvailable ? frontend : null,
    backendAvailable ? backend : null,
  ]
    .filter((release): release is RuntimeReleaseStatus => release !== null)
    .filter(
      (release, index, all) =>
        all.findIndex(
          (candidate) => candidate.release_url === release.release_url,
        ) === index,
    )
  const frontendPhase =
    updates.startFrontendUpdate.data?.phase ?? frontend?.phase
  const backendPhase =
    updates.startBackendUpdate.data?.phase ?? updates.backendUpdate.data?.phase

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Updates available</DialogTitle>
          <DialogDescription>
            Review what changed before updating this Oore installation.
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-3 sm:grid-cols-2">
          {frontendAvailable ? (
            <RuntimeUpdateCard
              name="Frontend"
              release={frontend}
              phase={frontendPhase}
              pending={updates.startFrontendUpdate.isPending}
              managed={frontend.managed_service}
              onUpdate={() =>
                updates.startFrontendUpdate.mutate(undefined, {
                  onSuccess: () =>
                    toast.success(
                      'Frontend update started. The UI will reconnect after restart.',
                    ),
                  onError: (error) => toast.error(error.message),
                })
              }
            />
          ) : null}
          {backendAvailable ? (
            <RuntimeUpdateCard
              name="Backend"
              release={backend}
              phase={backendPhase}
              pending={updates.startBackendUpdate.isPending}
              managed={updates.backendUpdate.data?.managed_service === true}
              onUpdate={() =>
                updates.startBackendUpdate.mutate(undefined, {
                  onSuccess: () =>
                    toast.success(
                      'Backend update started. Readiness will recover after restart.',
                    ),
                  onError: (error) => toast.error(error.message),
                })
              }
            />
          ) : null}
        </div>

        <div className="flex flex-col gap-4">
          {releases.map((release) => {
            const notes = formatReleaseNotes(release.release_notes)
            return (
              <section
                key={release.release_url}
                className="flex flex-col gap-2"
              >
                <div className="flex items-center justify-between gap-3">
                  <div>
                    <p className="text-sm font-medium">
                      {release.release_name}
                    </p>
                    <p className="text-xs text-muted-foreground">
                      What changed in {release.latest_version}
                    </p>
                  </div>
                  <Badge
                    variant="link"
                    render={
                      <a
                        href={release.changelog_url}
                        aria-label={`Open the full changelog for ${release.latest_version}`}
                        target="_blank"
                        rel="noopener noreferrer"
                      />
                    }
                  >
                    Full changelog
                    <HugeiconsIcon
                      icon={ArrowUpRight01Icon}
                      data-icon="inline-end"
                    />
                  </Badge>
                </div>
                <ScrollArea className="max-h-52 border bg-muted/30">
                  <p className="whitespace-pre-wrap p-4 text-sm leading-6">
                    {notes ||
                      'No release notes were published for this version.'}
                  </p>
                </ScrollArea>
              </section>
            )
          })}
        </div>
      </DialogContent>
    </Dialog>
  )
}
