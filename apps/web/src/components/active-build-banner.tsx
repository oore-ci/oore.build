import { Link } from '@tanstack/solid-router'
import { Loading03Icon, TimeQuarter02Icon } from '@hugeicons/core-free-icons'
import type { Build } from '@/lib/types'
import { Badge } from '@/components/ui/badge'
import { HugeIcon } from '@/components/huge-icon'
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

export default function ActiveBuildBanner(props: ActiveBuildBannerProps) {
  const startTime = () => props.build.started_at ?? props.build.queued_at

  return (
    <Link
      to="/builds/$buildId"
      params={{ buildId: props.build.id }}
      class="group flex items-center gap-3 border border-border/60 bg-card px-4 py-2.5 text-sm transition-colors hover:border-primary/30 hover:bg-primary/5"
    >
      {props.build.status === 'running' ? (
        <HugeIcon
          icon={Loading03Icon}
          size={14}
          class="shrink-0 animate-spin text-info"
        />
      ) : (
        <HugeIcon
          icon={TimeQuarter02Icon}
          size={14}
          class="shrink-0 text-muted-foreground"
        />
      )}

      <span class="font-mono text-xs font-medium">#{props.build.build_number}</span>

      <Badge variant={getStatusVariant(props.build.status)} class="text-[10px]">
        {props.build.status}
      </Badge>

      <span class="truncate text-xs text-muted-foreground">
        {props.build.branch ?? 'n/a'}
      </span>

      <span class="ml-auto shrink-0 font-mono text-xs text-muted-foreground">
        {elapsed(startTime())}
      </span>
    </Link>
  )
}
