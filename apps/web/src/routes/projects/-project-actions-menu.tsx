import { Link } from '@tanstack/react-router'
import {
  ArrowRight as ArrowRight01Icon,
  CircleEllipsis as MoreHorizontalCircle01Icon,
  Settings as Settings01Icon,
} from 'lucide-react'

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
        <MoreHorizontalCircle01Icon />
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
          <ArrowRight01Icon />
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
            <Settings01Icon />
            Project settings
          </DropdownMenuItem>
        ) : null}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
