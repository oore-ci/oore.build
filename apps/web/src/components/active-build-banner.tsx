import { Link } from '@tanstack/react-router'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import {
  LoaderCircle ,
  Clock ,
} from 'lucide-react'

import type { Build } from '@/lib/types'
import { Badge } from '@/components/ui/badge'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemMedia,
  ItemTitle,
} from '@/components/ui/item'
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
    <Item
      variant="outline"
      size="xs"
      render={
        <Link
          to="/builds/$buildId"
          params={{ buildId: build.id }}
        />
      }
    >
      <ItemMedia variant="icon">
        {build.status === 'running' ? (
          <DynamicLucideIcon
            icon={LoaderCircle}
            className="animate-spin text-info"
          />
        ) : (
          <DynamicLucideIcon
            icon={Clock}
            className="text-muted-foreground"
          />
        )}
      </ItemMedia>
      <ItemContent>
        <ItemTitle>
          #{build.build_number}
          <Badge variant={getStatusVariant(build.status)}>{build.status}</Badge>
        </ItemTitle>
        <ItemDescription>{build.branch ?? 'n/a'}</ItemDescription>
      </ItemContent>
      <ItemActions>{elapsed(startTime)}</ItemActions>
    </Item>
  )
}
