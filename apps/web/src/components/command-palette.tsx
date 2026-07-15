import { useCallback, useMemo } from 'react'
import { useNavigate } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  CommandLineIcon,
  ComputerIcon,
  FolderLibraryIcon,
  Home01Icon,
  LinkSquare01Icon,
  Settings01Icon,
  UserMultiple02Icon,
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
import { useAuthStore } from '@/stores/auth-store'
import { useUiStore } from '@/stores/ui-store'
import { useHasPermission } from '@/hooks/use-permissions'
import type { Project } from '@/lib/types'

const EMPTY_PROJECTS: Array<Project> = []

interface PaletteItem {
  id: string
  label: string
  icon: typeof Home01Icon
  action: () => void
  keywords?: string
}

export default function CommandPalette() {
  const open = useUiStore((state) => state.commandPaletteOpen)
  const setOpen = useUiStore((state) => state.setCommandPaletteOpen)
  const navigate = useNavigate()
  const authUser = useAuthStore((s) => s.user)

  const isAdmin = authUser?.role === 'owner' || authUser?.role === 'admin'
  const isQaViewer = authUser?.role === 'qa_viewer'
  const canWriteProjects = useHasPermission('projects', 'write')

  const { data: projectsData } = useProjects(
    { limit: 50 },
    { enabled: !isQaViewer },
  )
  const projects = projectsData?.projects ?? EMPTY_PROJECTS

  const go = useCallback(
    (to: string) => {
      setOpen(false)
      void navigate({ to })
    },
    [navigate, setOpen],
  )

  const navItems = useMemo<Array<PaletteItem>>(
    () => [
      ...(!isQaViewer
        ? [
            {
              id: 'nav-dashboard',
              label: 'Dashboard',
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
    ],
    [go, isQaViewer],
  )

  const adminItems = useMemo<Array<PaletteItem>>(
    () =>
      isAdmin
        ? [
            {
              id: 'nav-users',
              label: 'Users',
              icon: UserMultiple02Icon,
              action: () => go('/settings/users'),
              keywords: 'team members invite',
            },
            {
              id: 'nav-runners',
              label: 'Runners',
              icon: ComputerIcon,
              action: () => go('/settings/runners'),
              keywords: 'machines agents workers',
            },
            {
              id: 'nav-sources',
              label: 'Sources',
              icon: LinkSquare01Icon,
              action: () => go('/settings/integrations'),
              keywords: 'github gitlab integrations',
            },
            {
              id: 'nav-preferences',
              label: 'Preferences',
              icon: Settings01Icon,
              action: () => go('/settings/preferences'),
              keywords: 'settings config',
            },
          ]
        : [],
    [go, isAdmin],
  )

  const actionItems = useMemo<Array<PaletteItem>>(
    () =>
      canWriteProjects
        ? [
            {
              id: 'action-new-project',
              label: 'Create new project',
              icon: FolderLibraryIcon,
              action: () => go('/projects?openCreate=1'),
              keywords: 'add new project create',
            },
          ]
        : [],
    [go, canWriteProjects],
  )

  const projectItems = useMemo<Array<PaletteItem>>(
    () =>
      (isQaViewer ? [] : projects).map((project) => ({
        id: `project-${project.id}`,
        label: project.name,
        icon: FolderLibraryIcon,
        action: () => go(`/projects/${project.id}`),
        keywords: project.description ?? '',
      })),
    [go, isQaViewer, projects],
  )

  return (
    <CommandDialog open={open} onOpenChange={setOpen}>
      <Command>
        <CommandInput
          placeholder={
            isQaViewer
              ? 'Search builds and pages...'
              : 'Search projects, pages, actions...'
          }
        />
        <CommandList>
          <CommandEmpty>No results found.</CommandEmpty>
          <CommandGroup heading="Navigation">
            {navItems.map((item) => (
              <CommandItem
                key={item.id}
                value={item.label}
                keywords={item.keywords ? [item.keywords] : undefined}
                onSelect={() => item.action()}
              >
                <HugeiconsIcon
                  icon={item.icon}
                  size={16}
                  className="text-muted-foreground"
                />
                {item.label}
              </CommandItem>
            ))}
          </CommandGroup>
          {adminItems.length > 0 ? (
            <>
              <CommandSeparator />
              <CommandGroup heading="Admin">
                {adminItems.map((item) => (
                  <CommandItem
                    key={item.id}
                    value={item.label}
                    keywords={item.keywords ? [item.keywords] : undefined}
                    onSelect={() => item.action()}
                  >
                    <HugeiconsIcon
                      icon={item.icon}
                      size={16}
                      className="text-muted-foreground"
                    />
                    {item.label}
                  </CommandItem>
                ))}
              </CommandGroup>
            </>
          ) : null}
          {actionItems.length > 0 ? (
            <>
              <CommandSeparator />
              <CommandGroup heading="Actions">
                {actionItems.map((item) => (
                  <CommandItem
                    key={item.id}
                    value={item.label}
                    keywords={item.keywords ? [item.keywords] : undefined}
                    onSelect={() => item.action()}
                  >
                    <HugeiconsIcon
                      icon={item.icon}
                      size={16}
                      className="text-muted-foreground"
                    />
                    {item.label}
                  </CommandItem>
                ))}
              </CommandGroup>
            </>
          ) : null}
          {projectItems.length > 0 ? (
            <>
              <CommandSeparator />
              <CommandGroup heading="Projects">
                {projectItems.map((item) => (
                  <CommandItem
                    key={item.id}
                    value={item.label}
                    keywords={item.keywords ? [item.keywords] : undefined}
                    onSelect={() => item.action()}
                  >
                    <HugeiconsIcon
                      icon={item.icon}
                      size={16}
                      className="text-muted-foreground"
                    />
                    {item.label}
                  </CommandItem>
                ))}
              </CommandGroup>
            </>
          ) : null}
        </CommandList>
      </Command>
    </CommandDialog>
  )
}
