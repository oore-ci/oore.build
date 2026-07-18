import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowRight01Icon,
  MoreHorizontalCircle01Icon,
} from '@hugeicons/core-free-icons'

import type { Build } from '@/lib/types'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'

export default function BuildActionsMenu({
  build,
  open,
  onOpenChange,
}: {
  build: Build
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  return (
    <DropdownMenu open={open} onOpenChange={onOpenChange}>
      <DropdownMenuTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={`Actions for build ${build.build_number}`}
            title="Build actions"
          />
        }
      >
        <HugeiconsIcon icon={MoreHorizontalCircle01Icon} />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem
          render={<Link to="/builds/$buildId" params={{ buildId: build.id }} />}
        >
          <HugeiconsIcon icon={ArrowRight01Icon} />
          Open build
        </DropdownMenuItem>
        <DropdownMenuItem
          render={
            <Link
              to="/projects/$projectId"
              params={{ projectId: build.project_id }}
              search={{ tab: 'builds' }}
            />
          }
        >
          Open project builds
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
