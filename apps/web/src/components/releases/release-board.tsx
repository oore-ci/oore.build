import { useMemo } from 'react'
import { Link } from '@tanstack/react-router'

import type { ListReleasesResponse, Release } from '@oore/client/models'
import {
  DataTable,
  useDataTable,
  type DataTableColumnDef,
} from '@/components/data-table'
import { ReleaseActions } from '@/components/releases/release-actions'
import { ReleaseRollout } from '@/components/releases/release-rollout'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { useReleases } from '@/hooks/use-releases'
import { useTime } from '@/hooks/use-time'
import {
  getReleaseHealthVariant,
  LIVE_RELEASE_STATES,
  RELEASE_HEALTH_LABEL,
  RELEASE_STORE_LABEL,
  releaseAppName,
  releaseNextStep,
  releaseVersionLabel,
} from '@/lib/releases'

const LIVE_STATE_QUERY = LIVE_RELEASE_STATES.join(',')

function selectReleases({ releases }: ListReleasesResponse) {
  return releases
}

function getReleaseColumns(
  nowSeconds: number,
): Array<DataTableColumnDef<Release>> {
  return [
    {
      id: 'app',
      header: 'App',
      cell: ({ row }) => (
        <Link
          to="/releases/$releaseId"
          params={{ releaseId: row.original.id }}
          className="font-medium underline-offset-4 hover:underline"
        >
          {releaseAppName(row.original)}
        </Link>
      ),
      enableHiding: false,
      enableSorting: false,
    },
    {
      id: 'rollout',
      header: 'Rollout',
      cell: ({ row }) => <ReleaseRollout release={row.original} />,
      enableHiding: false,
      enableSorting: false,
    },
    {
      id: 'store',
      header: 'Store',
      cell: ({ row }) => (
        <>
          <p>{RELEASE_STORE_LABEL[row.original.store]}</p>
          <p className="text-xs text-muted-foreground">{row.original.track}</p>
        </>
      ),
      enableSorting: false,
    },
    {
      id: 'version',
      header: 'Version',
      cell: ({ row }) => (
        <span className="font-mono text-xs tabular-nums">
          {releaseVersionLabel(row.original)}
        </span>
      ),
      enableSorting: false,
    },
    {
      id: 'next_step',
      header: 'Next step',
      cell: ({ row }) => (
        <span className="text-sm text-muted-foreground">
          {releaseNextStep(row.original, nowSeconds)}
        </span>
      ),
      enableSorting: false,
    },
    {
      id: 'health',
      header: 'Health',
      cell: ({ row }) => (
        <Badge variant={getReleaseHealthVariant(row.original.health)}>
          {RELEASE_HEALTH_LABEL[row.original.health]}
        </Badge>
      ),
      enableSorting: false,
    },
    {
      id: 'actions',
      header: () => <span className="sr-only">Actions</span>,
      cell: ({ row }) => <ReleaseActions release={row.original} />,
      enableHiding: false,
      enableSorting: false,
    },
  ]
}

/** Home's view of every release that is still moving or waiting on someone. */
export function ReleaseBoard() {
  const nowSeconds = useTime((time) => Math.floor(time / 60_000) * 60)
  const releasesQuery = useReleases(
    { state: LIVE_STATE_QUERY, limit: 50 },
    { select: selectReleases },
  )
  const releases = releasesQuery.data ?? []
  const columns = useMemo(() => getReleaseColumns(nowSeconds), [nowSeconds])
  const table = useDataTable({
    columns,
    data: releases,
    getRowId: (release) => release.id,
  })

  // Nothing live and nothing broken: stay out of the way.
  if (releasesQuery.isSuccess && releases.length === 0) return null

  return (
    <section className="flex flex-col gap-1" aria-labelledby="release-board">
      <h2
        id="release-board"
        className="text-sm font-medium text-muted-foreground"
      >
        Releases
      </h2>
      {releasesQuery.error ? (
        <Alert variant="destructive" className="mt-2">
          <AlertDescription className="flex items-center justify-between gap-3">
            <span>Releases could not be loaded.</span>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void releasesQuery.refetch()}
            >
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      ) : releasesQuery.isLoading ? (
        <Skeleton className="mt-2 h-32 w-full" />
      ) : (
        <DataTable table={table} emptyMessage="No live releases." />
      )}
    </section>
  )
}
