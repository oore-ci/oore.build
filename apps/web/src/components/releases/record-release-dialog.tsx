import { useState } from 'react'
import type { ReleaseStore } from '@oore/client/models'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import { useBuilds } from '@/hooks/use-builds'
import { useRecordRelease } from '@/hooks/use-release-freezes'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { buildVersionLabel } from '@/lib/build-identity'
import { relativeTime } from '@/lib/format-utils'
import { toast } from '@/lib/toast'

const STORES: Array<{ value: ReleaseStore; label: string }> = [
  { value: 'google_play', label: 'Google Play' },
  { value: 'app_store', label: 'App Store' },
]

function RecordDialog({
  projectId,
  onOpenChange,
}: {
  projectId: string
  onOpenChange: (open: boolean) => void
}) {
  const buildsQuery = useBuilds({
    project_id: projectId,
    status: 'succeeded',
    sort: 'created_at',
    direction: 'desc',
    limit: 20,
  })
  const builds = buildsQuery.data?.builds ?? []
  const [buildId, setBuildId] = useState<string | null>(null)
  const [store, setStore] = useState<ReleaseStore>('google_play')
  const [track, setTrack] = useState('production')
  const record = useRecordRelease()
  const chosen = builds.find((build) => build.id === buildId) ?? builds[0]
  const items = Object.fromEntries(
    builds.map((build) => [
      build.id,
      `#${build.build_number}${buildVersionLabel(build) ? ` · ${buildVersionLabel(build)}` : ''} · ${relativeTime(build.created_at)}`,
    ]),
  )

  function submit() {
    if (!chosen) return
    record.mutate(
      {
        build_id: chosen.id,
        store,
        track: store === 'app_store' ? 'app_store' : track.trim(),
        version_name: chosen.version_name ?? null,
        version_code: chosen.version_code ?? null,
        state: 'complete',
        rollout_fraction: 1,
        reason: 'Uploaded by hand in the store console',
      },
      {
        onSuccess: () => {
          toast.success(`Recorded build #${chosen.build_number} as released`)
          onOpenChange(false)
        },
        onError: (error) =>
          toast.error(
            getApiErrorMessage(error, {
              build_not_succeeded: 'Only a succeeded build can be recorded.',
            }),
          ),
      },
    )
  }

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Record a release</DialogTitle>
          <DialogDescription>
            For a build you uploaded yourself in Play Console or App Store
            Connect. Oore records it as out to everyone on that track, so the
            app&apos;s releases stay complete. Nothing is sent to the store.
          </DialogDescription>
        </DialogHeader>
        {buildsQuery.isSuccess && builds.length === 0 ? (
          <Text variant="muted">This app has no succeeded builds yet.</Text>
        ) : (
          <Stack gap={4}>
            <Stack gap={2}>
              <Label htmlFor="record-build">Build</Label>
              <Select
                value={chosen?.id ?? null}
                onValueChange={(value) => setBuildId(value)}
                items={items}
              >
                <SelectTrigger id="record-build" className="w-full">
                  <SelectValue placeholder="Choose a build" />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    {builds.map((build) => (
                      <SelectItem key={build.id} value={build.id}>
                        {items[build.id]}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
            </Stack>
            <Stack gap={2}>
              <Label>Store</Label>
              <Row gap={2}>
                {STORES.map((option) => (
                  <Button
                    key={option.value}
                    size="sm"
                    variant={store === option.value ? 'default' : 'outline'}
                    aria-pressed={store === option.value}
                    onClick={() => setStore(option.value)}
                  >
                    {option.label}
                  </Button>
                ))}
              </Row>
            </Stack>
            {store === 'google_play' ? (
              <Stack gap={2}>
                <Label htmlFor="record-track">Track</Label>
                <Input
                  id="record-track"
                  value={track}
                  placeholder="production"
                  onChange={(event) => setTrack(event.target.value)}
                />
              </Stack>
            ) : null}
          </Stack>
        )}
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            disabled={
              !chosen ||
              record.isPending ||
              (store === 'google_play' && !track.trim())
            }
            onClick={submit}
          >
            {record.isPending ? 'Recording...' : 'Record'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

export function RecordReleaseButton({ projectId }: { projectId: string }) {
  const [open, setOpen] = useState(false)
  return (
    <>
      <Button size="sm" variant="outline" onClick={() => setOpen(true)}>
        I uploaded a build myself
      </Button>
      {open ? (
        <RecordDialog projectId={projectId} onOpenChange={setOpen} />
      ) : null}
    </>
  )
}
