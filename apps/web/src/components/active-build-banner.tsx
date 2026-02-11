import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Loading03Icon, TimeQuarter02Icon } from '@hugeicons/core-free-icons'

import type { Build } from '@/lib/types'
import { Badge } from '@/components/ui/badge'
import { getStatusVariant } from '@/lib/status-variants'

function elapsed(startEpoch: number): string {
  const seconds = Math.floor(Date.now() / 1000) - startEpoch
  if (seconds < 60) return `${seconds}s`
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m ${seconds % 60}s`
  const hours = Math.floor(minutes / 60)
  return `${hours}h ${minutes % 60}m`
}

interface ActiveBuildBannerProps {
  build: Build
}

export default function ActiveBuildBanner({ build }: ActiveBuildBannerProps) {
  const startTime = build.started_at ?? build.queued_at

  return (
    <Link
      to="/builds/$buildId"
      params={{ buildId: build.id }}
      className="group flex items-center gap-3 border border-border/60 bg-card px-4 py-2.5 text-sm transition-colors hover:border-primary/30 hover:bg-primary/5"
    >
      {build.status === 'running' ? (
        <HugeiconsIcon
          icon={Loading03Icon}
          size={14}
          className="shrink-0 animate-spin text-info"
        />
      ) : (
        <HugeiconsIcon
          icon={TimeQuarter02Icon}
          size={14}
          className="shrink-0 text-muted-foreground"
        />
      )}

      <span className="font-mono text-xs font-medium">
        #{build.build_number}
      </span>

      <Badge variant={getStatusVariant(build.status)} className="text-[10px]">
        {build.status}
      </Badge>

      <span className="truncate text-xs text-muted-foreground">
        {build.branch ?? 'n/a'}
      </span>

      <span className="ml-auto shrink-0 font-mono text-xs text-muted-foreground">
        {elapsed(startTime)}
      </span>
    </Link>
  )
}
