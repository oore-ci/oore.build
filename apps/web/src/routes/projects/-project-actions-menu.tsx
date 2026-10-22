import { Link } from '@tanstack/react-router'

import type { Project } from '@oore/client/models'
import { hasProjectPermission, useHasPermission } from '@/hooks/use-permissions'
import { DataTableRowActionsTrigger } from '@/components/data-table'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
} from '@/components/ui/dropdown-menu'

export default function ProjectActionsMenu({
  canManage,
  project,
}: {
  canManage: boolean
  project: Project
}) {
  const canTriggerGlobally = useHasPermission('builds:write')
  const canRunBuild =
    canTriggerGlobally &&
    hasProjectPermission(project.current_user_role, 'builds:write')

  // The project name already opens the project, so the menu holds only what
  // the row cannot: start a build, jump to its builds, or manage it.
  return (
    <DropdownMenu>
      <DataTableRowActionsTrigger label={`Actions for ${project.name}`} />
      <DropdownMenuContent align="end" className="w-44">
        <DropdownMenuGroup>
          <DropdownMenuLabel>Actions</DropdownMenuLabel>
          {canRunBuild ? (
            <DropdownMenuItem
              render={
                <Link
                  to="/projects/$projectId"
                  params={{ projectId: project.id }}
                  search={{ run: '1' }}
                />
              }
            >
              Run build
            </DropdownMenuItem>
          ) : null}
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
              Project settings
            </DropdownMenuItem>
          ) : null}
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
