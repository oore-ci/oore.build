import { Link } from '@tanstack/react-router'

import RepositoryAvatar from '@/components/repository-avatar'
import type { Build } from '@/lib/types'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemMedia,
  ItemTitle,
} from '@/components/ui/item'

interface ActiveBuildBannerProps {
  build: Build
}

export default function ActiveBuildBanner({ build }: ActiveBuildBannerProps) {
  const projectName = build.context?.project_name ?? build.project_id

  return (
    <Item
      variant="outline"
      size="xs"
      render={<Link to="/builds/$buildId" params={{ buildId: build.id }} />}
    >
      <ItemMedia>
        <RepositoryAvatar
          fullName={build.context?.repository_full_name ?? projectName}
          avatarUrl={build.context?.project_avatar_url}
          size="sm"
        />
      </ItemMedia>
      <ItemContent>
        <ItemTitle>{projectName}</ItemTitle>
      </ItemContent>
      <ItemActions>#{build.build_number}</ItemActions>
    </Item>
  )
}
