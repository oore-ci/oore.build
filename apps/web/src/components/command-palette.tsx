import { useNavigate } from '@tanstack/react-router'
import { useTheme } from 'next-themes'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ComputerIcon,
  FolderAddIcon,
  Moon02Icon,
  PlayIcon,
  Sun03Icon,
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
import { WORKSPACE_ITEMS } from '@/components/nav-main'
import { useBuilds } from '@/hooks/use-builds'
import { useProjects } from '@/hooks/use-projects'
import { settingsGroupsForRole } from '@/components/settings/settings-navigation'
import { useAuthStore } from '@/stores/auth-store'
import { useHasPermissions } from '@/hooks/use-permissions'
import type { Build, Project } from '@oore/client/models'

const EMPTY_PROJECTS: Array<Project> = []
const EMPTY_BUILDS: Array<Build> = []
const PROJECT_LIMIT = 50
const RECENT_BUILD_LIMIT = 20

const NAV_KEYWORDS = new Map([
  ['/', 'home overview dashboard'],
  ['/projects', 'repositories repos apps'],
  ['/builds', 'queue history runs'],
])

interface PaletteItem {
  id: string
  label: string
  icon: (typeof WORKSPACE_ITEMS)[number]['icon']
  action: () => void
  keywords?: Array<string>
}

function PaletteGroup({
  heading,
  items,
  separated = true,
}: {
  heading: string
  items: Array<PaletteItem>
  separated?: boolean
}) {
  if (items.length === 0) return null

  return (
    <>
      {separated ? <CommandSeparator /> : null}
      <CommandGroup heading={heading}>
        {items.map((item) => (
          <CommandItem
            key={item.id}
            value={`${item.label} ${item.id}`}
            keywords={item.keywords}
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
  )
}

export default function CommandPalette({
  open,
  onOpenChange,
  onRunBuild,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  onRunBuild: () => void
}) {
  const navigate = useNavigate()
  const { setTheme } = useTheme()
  const authUser = useAuthStore((s) => s.user)

  const isQaViewer = authUser?.role === 'qa_viewer'
  const [canWriteProjects, canTriggerBuild] = useHasPermissions([
    'projects:write',
    'builds:write',
  ])

  const {
    data: projectsData,
    isLoading,
    error,
  } = useProjects({ limit: PROJECT_LIMIT }, { enabled: open && !isQaViewer })
  const projects = projectsData?.projects ?? EMPTY_PROJECTS
  const { data: buildsData } = useBuilds(
    { limit: RECENT_BUILD_LIMIT },
    { enabled: open && !isQaViewer, refetchInterval: false },
  )
  const builds = buildsData?.builds ?? EMPTY_BUILDS

  function close() {
    onOpenChange(false)
  }

  function go(to: string) {
    close()
    void navigate({ to })
  }

  const navItems: Array<PaletteItem> = WORKSPACE_ITEMS.filter(
    (item) => !isQaViewer || item.to === '/builds',
  ).map((item) => ({
    id: `nav-${item.to}`,
    label: item.title,
    icon: item.icon,
    action: () => go(item.to),
    keywords: [NAV_KEYWORDS.get(item.to) ?? ''],
  }))

  const settingsItems: Array<PaletteItem> = settingsGroupsForRole(
    authUser?.role,
  )
    .flatMap((group) => group.items)
    .map((item) => ({
      id: item.to,
      label: item.title,
      icon: item.icon,
      action: () => go(item.to),
      keywords: item.description ? [item.description] : undefined,
    }))

  const actionItems: Array<PaletteItem> = [
    ...(canTriggerBuild && !isQaViewer
      ? [
          {
            id: 'action-run-build',
            label: 'Run build…',
            icon: PlayIcon,
            action: () => {
              close()
              onRunBuild()
            },
            keywords: ['start trigger new build manual'],
          },
        ]
      : []),
    ...(canWriteProjects
      ? [
          {
            id: 'action-new-project',
            label: 'Create project…',
            icon: FolderAddIcon,
            action: () => go('/projects?openCreate=1'),
            keywords: ['add new project create'],
          },
        ]
      : []),
  ]

  const themeItems: Array<PaletteItem> = [
    {
      id: 'theme-light',
      label: 'Use light theme',
      icon: Sun03Icon,
      action: () => {
        setTheme('light')
        close()
      },
      keywords: ['appearance theme light mode'],
    },
    {
      id: 'theme-dark',
      label: 'Use dark theme',
      icon: Moon02Icon,
      action: () => {
        setTheme('dark')
        close()
      },
      keywords: ['appearance theme dark mode'],
    },
    {
      id: 'theme-system',
      label: 'Use system theme',
      icon: ComputerIcon,
      action: () => {
        setTheme('system')
        close()
      },
      keywords: ['appearance theme system auto'],
    },
  ]

  const buildIcon =
    WORKSPACE_ITEMS.find((item) => item.to === '/builds')?.icon ??
    WORKSPACE_ITEMS[0].icon
  const buildItems: Array<PaletteItem> = builds.map((build) => ({
    id: `build-${build.id}`,
    label: [
      `Build #${build.build_number}`,
      build.context?.project_name,
      build.context?.pipeline_name,
    ]
      .filter(Boolean)
      .join(' · '),
    icon: buildIcon,
    action: () => go(`/builds/${build.id}`),
    keywords: [`#${build.build_number}`, String(build.build_number)],
  }))

  const projectIcon =
    WORKSPACE_ITEMS.find((item) => item.to === '/projects')?.icon ??
    WORKSPACE_ITEMS[0].icon
  const projectItems: Array<PaletteItem> = (isQaViewer ? [] : projects).map(
    (project) => ({
      id: `project-${project.id}`,
      label: project.name,
      icon: projectIcon,
      action: () => go(`/projects/${project.id}`),
      keywords: project.description ? [project.description] : undefined,
    }),
  )

  const projectTotal = projectsData?.total ?? 0
  const projectStatus = isQaViewer
    ? null
    : error
      ? 'Projects could not be loaded. Open Projects to retry.'
      : isLoading
        ? 'Loading projects…'
        : projectTotal > projects.length
          ? `Showing ${projects.length} of ${projectTotal} projects. Open Projects to search them all.`
          : null

  return (
    <CommandDialog
      open={open}
      onOpenChange={onOpenChange}
      title="Quick open"
      description="Open a page, project, build or action."
    >
      <Command>
        <CommandInput
          aria-label="Find a page, project, build or action"
          placeholder={
            isQaViewer
              ? 'Find a page or action…'
              : 'Find a project, build #, page or action…'
          }
        />
        <CommandList>
          <CommandEmpty>No results found.</CommandEmpty>
          <PaletteGroup heading="Go to" items={navItems} separated={false} />
          <PaletteGroup heading="Actions" items={actionItems} />
          <PaletteGroup heading="Recent builds" items={buildItems} />
          <PaletteGroup heading="Projects" items={projectItems} />
          <PaletteGroup heading="Settings" items={settingsItems} />
          <PaletteGroup heading="Theme" items={themeItems} />
        </CommandList>
        {projectStatus ? (
          <p
            className="border-t px-3 py-2 text-xs text-muted-foreground"
            role="status"
          >
            {projectStatus}
          </p>
        ) : null}
      </Command>
    </CommandDialog>
  )
}
