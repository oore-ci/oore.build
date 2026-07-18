import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ArrowRight01Icon,
  MoreHorizontalCircle01Icon,
  Settings01Icon,
} from '@hugeicons/core-free-icons'

import type { Project } from '@/lib/types'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'

export default function ProjectActionsMenu({
  canManage,
  open,
  onOpenChange,
  project,
}: {
  canManage: boolean
  open: boolean
  onOpenChange: (open: boolean) => void
  project: Project
}) {
  return (
    <DropdownMenu open={open} onOpenChange={onOpenChange}>
      <DropdownMenuTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={`Actions for ${project.name}`}
            title="Project actions"
          />
        }
      >
        <HugeiconsIcon icon={MoreHorizontalCircle01Icon} />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem
          render={
            <Link
              to="/projects/$projectId"
              params={{ projectId: project.id }}
            />
          }
        >
          <HugeiconsIcon icon={ArrowRight01Icon} />
          Open project
        </DropdownMenuItem>
        <DropdownMenuItem
          render={
            <Link
              to="/projects/$projectId"
              params={{ projectId: project.id }}
              search={{ tab: 'builds' }}
            />
          }
        >
          View builds
        </DropdownMenuItem>
        {canManage ? (
          <DropdownMenuItem
            render={
              <Link
                to="/projects/$projectId"
                params={{ projectId: project.id }}
                search={{ tab: 'settings' }}
              />
            }
          >
            <HugeiconsIcon icon={Settings01Icon} />
            Project settings
          </DropdownMenuItem>
        ) : null}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
