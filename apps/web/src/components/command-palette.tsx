import { useNavigate } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  CommandLineIcon,
  FolderLibraryIcon,
  Home01Icon,
} from '@hugeicons/core-free-icons'

import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
} from '@/components/ui/command'
import { useProjects } from '@/hooks/use-projects'
import { settingsGroupsForRole } from '@/components/settings/settings-navigation'
import { useAuthStore } from '@/stores/auth-store'
import { useHasPermission } from '@/hooks/use-permissions'
import type { Project } from '@oore/client/models'

const EMPTY_PROJECTS: Array<Project> = []

interface PaletteItem {
  id: string
  label: string
  icon: typeof Home01Icon
  action: () => void
  keywords?: string
}

export default function CommandPalette({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const navigate = useNavigate()
  const authUser = useAuthStore((s) => s.user)

  const isQaViewer = authUser?.role === 'qa_viewer'
  const canWriteProjects = useHasPermission('projects:write')

  const {
    data: projectsData,
    isLoading,
    error,
  } = useProjects({ limit: 50 }, { enabled: open && !isQaViewer })
  const projects = projectsData?.projects ?? EMPTY_PROJECTS

  function go(to: string) {
    onOpenChange(false)
    void navigate({ to })
  }

  const navItems: Array<PaletteItem> = [
    ...(!isQaViewer
      ? [
          {
            id: 'nav-dashboard',
            label: 'Home',
            icon: Home01Icon,
            action: () => go('/'),
            keywords: 'home overview',
          },
          {
            id: 'nav-projects',
            label: 'Projects',
            icon: FolderLibraryIcon,
            action: () => go('/projects'),
            keywords: 'repositories repos',
          },
        ]
      : []),
    {
      id: 'nav-builds',
      label: 'Builds',
      icon: CommandLineIcon,
      action: () => go('/builds'),
      keywords: 'queue history runs',
    },
  ]

  const adminItems: Array<PaletteItem> = settingsGroupsForRole(authUser?.role)
    .flatMap((group) => group.items)
    .map((item) => ({
      id: item.to,
      label: item.title,
      icon: item.icon,
      action: () => go(item.to),
      keywords: item.description,
    }))

  const actionItems: Array<PaletteItem> = canWriteProjects
    ? [
        {
          id: 'action-new-project',
          label: 'Create new project',
          icon: FolderLibraryIcon,
          action: () => go('/projects?openCreate=1'),
          keywords: 'add new project create',
        },
      ]
    : []

  const projectItems: Array<PaletteItem> = (isQaViewer ? [] : projects).map(
    (project) => ({
      id: `project-${project.id}`,
      label: project.name,
      icon: FolderLibraryIcon,
      action: () => go(`/projects/${project.id}`),
      keywords: project.description ?? '',
    }),
  )

  return (
    <CommandDialog
      open={open}
      onOpenChange={onOpenChange}
      title="Quick open"
      description="Open a page, project, or action."
    >
      <Command>
        <CommandInput
          aria-label="Find a page, project, or action"
          placeholder={
            isQaViewer
              ? 'Find a page...'
              : 'Find recent projects, pages, actions...'
          }
        />
        {!isQaViewer ? (
          <p className="px-3 py-2 text-xs text-muted-foreground" role="status">
            {error
              ? 'Projects could not be loaded. Open Projects to retry.'
              : isLoading
                ? 'Loading recent projects…'
                : 'Includes up to 50 recent projects. Open Projects to search all.'}
          </p>
        ) : null}
        <CommandList>
          <CommandEmpty>No results found.</CommandEmpty>
          <CommandGroup heading="Navigation">
            {navItems.map((item) => {
              const Icon = item.icon

              return (
                <CommandItem
                  key={item.id}
                  value={item.label}
                  keywords={item.keywords ? [item.keywords] : undefined}
                  onSelect={() => item.action()}
                >
                  <HugeiconsIcon
                    icon={Icon}
                    size={16}
                    className="text-muted-foreground"
                  />
                  {item.label}
                </CommandItem>
              )
            })}
          </CommandGroup>
          {adminItems.length > 0 ? (
            <>
              <CommandSeparator />
              <CommandGroup heading="Settings">
                {adminItems.map((item) => {
                  const Icon = item.icon

                  return (
                    <CommandItem
                      key={item.id}
                      value={item.label}
                      keywords={item.keywords ? [item.keywords] : undefined}
                      onSelect={() => item.action()}
                    >
                      <HugeiconsIcon
                        icon={Icon}
                        size={16}
                        className="text-muted-foreground"
                      />
                      {item.label}
                    </CommandItem>
                  )
                })}
              </CommandGroup>
            </>
          ) : null}
          {actionItems.length > 0 ? (
            <>
              <CommandSeparator />
              <CommandGroup heading="Actions">
                {actionItems.map((item) => {
                  const Icon = item.icon

                  return (
                    <CommandItem
                      key={item.id}
                      value={item.label}
                      keywords={item.keywords ? [item.keywords] : undefined}
                      onSelect={() => item.action()}
                    >
                      <HugeiconsIcon
                        icon={Icon}
                        size={16}
                        className="text-muted-foreground"
                      />
                      {item.label}
                    </CommandItem>
                  )
                })}
              </CommandGroup>
            </>
          ) : null}
          {projectItems.length > 0 ? (
            <>
              <CommandSeparator />
              <CommandGroup heading="Projects">
                {projectItems.map((item) => {
                  const Icon = item.icon

                  return (
                    <CommandItem
                      key={item.id}
                      value={item.label}
                      keywords={item.keywords ? [item.keywords] : undefined}
                      onSelect={() => item.action()}
                    >
                      <HugeiconsIcon
                        icon={Icon}
                        size={16}
                        className="text-muted-foreground"
                      />
                      {item.label}
                    </CommandItem>
                  )
                })}
              </CommandGroup>
            </>
          ) : null}
        </CommandList>
      </Command>
    </CommandDialog>
  )
}
