import { useState } from 'react'
import { defaultFilter } from 'cmdk'
import { useNavigate } from '@tanstack/react-router'
import { useTheme } from 'next-themes'
import { detectPlatform } from '@tanstack/react-hotkeys'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  ComputerIcon,
  FolderAddIcon,
  Moon02Icon,
  PlayIcon,
  Refresh01Icon,
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
  CommandShortcut,
} from '@/components/ui/command'
import { WORKSPACE_ITEMS } from '@/components/nav-main'
import { useBuilds } from '@/hooks/use-builds'
import { useProject, useProjects } from '@/hooks/use-projects'
import { useRerunAndOpen } from '@/hooks/use-rerun-and-open'
import { settingsGroupsForRole } from '@/components/settings/settings-navigation'
import { useAuthStore } from '@/stores/auth-store'
import {
  hasProjectPermission,
  useHasPermissions,
} from '@/hooks/use-permissions'
import {
  SHORTCUTS,
  parseBuildNumberQuery,
  shortcutHint,
  type SequenceShortcutDefinition,
} from '@/lib/keyboard-shortcuts'
import { getStatusPresentation } from '@/lib/status-variants'
import { rankPaletteSections } from '@/lib/palette-ranking'
import { useCommandStore } from '@/stores/command-store'
import type { RunBuildTarget } from '@/stores/command-store'
import type { Build, Project } from '@oore/client/models'
import { Text } from '@/components/layout/text'

const EMPTY_PROJECTS: Array<Project> = []
const EMPTY_BUILDS: Array<Build> = []
const PROJECT_LIMIT = 50
const RECENT_BUILD_LIMIT = 20
const OPEN_BUILD_LIMIT = 5

const NAV_KEYWORDS = new Map([
  ['/', 'home overview dashboard'],
  ['/projects', 'repositories repos apps'],
  ['/builds', 'queue history runs'],
])

const NAV_SHORTCUTS = new Map<string, SequenceShortcutDefinition>([
  ['/', SHORTCUTS.goHome],
  ['/projects', SHORTCUTS.goProjects],
  ['/builds', SHORTCUTS.goBuilds],
])

interface PaletteItem {
  id: string
  label: string
  icon: (typeof WORKSPACE_ITEMS)[number]['icon']
  action: () => void
  keywords?: Array<string>
  /** Muted text at the end of the row: a shortcut or a status. */
  hint?: string
}

function paletteValue(item: PaletteItem) {
  return `${item.label} ${item.id}`
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
            value={paletteValue(item)}
            keywords={item.keywords}
            onSelect={() => item.action()}
          >
            <HugeiconsIcon
              icon={item.icon}
              size={16}
              className="text-muted-foreground"
            />
            <span className="min-w-0 truncate">{item.label}</span>
            {item.hint ? (
              <CommandShortcut className="tracking-normal">
                {item.hint}
              </CommandShortcut>
            ) : null}
          </CommandItem>
        ))}
      </CommandGroup>
    </>
  )
}

function buildLabel(build: Build) {
  return [
    `Build #${build.build_number}`,
    build.context?.project_name,
    build.context?.pipeline_name,
  ]
    .filter(Boolean)
    .join(' · ')
}

function isFailed(build: Build) {
  return build.status === 'failed' || build.status === 'timed_out'
}

export default function CommandPalette({
  open,
  onOpenChange,
  onRunBuild,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** Opens Run build, preselecting a pipeline when one is given. */
  onRunBuild: (target?: RunBuildTarget) => void
}) {
  const navigate = useNavigate()
  const { resolvedTheme, setTheme } = useTheme()
  const authUser = useAuthStore((s) => s.user)
  const pageCommands = useCommandStore((s) => s.pageCommands)
  const pageCommandsHeading = useCommandStore((s) => s.pageCommandsHeading)
  const [search, setSearch] = useState('')
  const platform = detectPlatform()

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

  const buildNumber = parseBuildNumberQuery(search)
  const { data: numberMatches } = useBuilds(
    { q: `#${buildNumber ?? 0}`, limit: OPEN_BUILD_LIMIT },
    {
      enabled: open && !isQaViewer && buildNumber !== null,
      refetchInterval: false,
    },
  )

  // The same check the build page makes before it offers Re-run.
  const lastFailed = builds.find(isFailed)
  const listedProject = projects.find(
    (project) => project.id === lastFailed?.project_id,
  )
  // Only fetched when the project is beyond the first page of projects.
  const lastFailedProject = useProject(
    lastFailed && !listedProject && !isLoading ? lastFailed.project_id : '',
  )
  const canRerunLastFailed =
    !!lastFailed &&
    canTriggerBuild &&
    hasProjectPermission(
      (listedProject ?? lastFailedProject.data?.project)?.current_user_role,
      'builds:write',
    )
  const rerunAndOpen = useRerunAndOpen()

  function close() {
    onOpenChange(false)
  }

  function go(to: string) {
    close()
    void navigate({ to })
  }

  const navItems: Array<PaletteItem> = WORKSPACE_ITEMS.filter(
    (item) => !isQaViewer || item.to === '/builds',
  ).map((item) => {
    const shortcut = NAV_SHORTCUTS.get(item.to)
    return {
      id: `nav-${item.to}`,
      label: item.title,
      icon: item.icon,
      action: () => go(item.to),
      keywords: [NAV_KEYWORDS.get(item.to) ?? ''],
      hint:
        shortcut && !isQaViewer ? shortcutHint(shortcut, platform) : undefined,
    }
  })

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

  const pageItems: Array<PaletteItem> = pageCommands.map((command) => ({
    id: `page-${command.id}`,
    label: command.label,
    icon: command.icon,
    hint: command.hint,
    keywords: command.keywords,
    action: () => {
      close()
      command.run()
    },
  }))

  const lastFailedIsOnPage =
    !!lastFailed &&
    pageCommands.some((command) => command.id === `rerun-${lastFailed.id}`)

  const actionItems: Array<PaletteItem> = [
    ...(canRerunLastFailed && lastFailed && !lastFailedIsOnPage
      ? [
          {
            id: 'action-rerun-last-failed',
            label: `Re-run last failed build · #${lastFailed.build_number}`,
            icon: Refresh01Icon,
            action: () => {
              close()
              rerunAndOpen.rerun(lastFailed.id)
            },
            keywords: [
              'rerun retry again failed',
              lastFailed.context?.project_name ?? '',
              lastFailed.context?.pipeline_name ?? '',
            ],
          },
        ]
      : []),
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
            keywords: ['start trigger new build manual run pipeline'],
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

  // Pipelines that built recently, where this user may start a build.
  const runnableProjectIds = new Set(
    projects
      .filter((project) =>
        hasProjectPermission(project.current_user_role, 'builds:write'),
      )
      .map((project) => project.id),
  )
  const runItems: Array<PaletteItem> =
    canTriggerBuild && !isQaViewer
      ? builds
          .filter(
            (build, index) =>
              runnableProjectIds.has(build.project_id) &&
              builds.findIndex(
                (other) => other.pipeline_id === build.pipeline_id,
              ) === index,
          )
          .map((build) => {
            const pipelineName = build.context?.pipeline_name ?? 'pipeline'
            return {
              id: `run-${build.pipeline_id}`,
              label: `Run ${pipelineName}`,
              icon: PlayIcon,
              hint: build.context?.project_name ?? undefined,
              action: () => {
                close()
                onRunBuild({
                  projectId: build.project_id,
                  pipelineId: build.pipeline_id,
                  pipelineName,
                })
              },
              keywords: [
                'run start trigger build',
                build.context?.project_name ?? '',
              ],
            }
          })
      : []

  function setThemeAndClose(theme: string) {
    setTheme(theme)
    close()
  }

  const themeItems: Array<PaletteItem> = [
    {
      id: 'theme-toggle',
      label:
        resolvedTheme === 'dark'
          ? 'Switch to light theme'
          : 'Switch to dark theme',
      icon: resolvedTheme === 'dark' ? Sun03Icon : Moon02Icon,
      action: () =>
        setThemeAndClose(resolvedTheme === 'dark' ? 'light' : 'dark'),
      keywords: ['toggle appearance theme dark light mode'],
      hint: shortcutHint(SHORTCUTS.toggleTheme, platform),
    },
    {
      id: 'theme-light',
      label: 'Use light theme',
      icon: Sun03Icon,
      action: () => setThemeAndClose('light'),
      keywords: ['appearance theme light mode'],
    },
    {
      id: 'theme-dark',
      label: 'Use dark theme',
      icon: Moon02Icon,
      action: () => setThemeAndClose('dark'),
      keywords: ['appearance theme dark mode'],
    },
    {
      id: 'theme-system',
      label: 'Use system theme',
      icon: ComputerIcon,
      action: () => setThemeAndClose('system'),
      keywords: ['appearance theme system auto'],
    },
  ]

  const buildIcon =
    WORKSPACE_ITEMS.find((item) => item.to === '/builds')?.icon ??
    WORKSPACE_ITEMS[0].icon
  const toBuildItem = (build: Build, prefix: string): PaletteItem => ({
    id: `${prefix}-${build.id}`,
    label: buildLabel(build),
    icon: buildIcon,
    hint: getStatusPresentation('build', build.status).label,
    action: () => go(`/builds/${build.id}`),
    keywords: [
      `#${build.build_number}`,
      String(build.build_number),
      `open ${build.build_number}`,
    ],
  })
  const buildItems = builds.map((build) => toBuildItem(build, 'build'))
  const recentIds = new Set(builds.map((build) => build.id))
  const openItems: Array<PaletteItem> =
    buildNumber === null
      ? []
      : (numberMatches?.builds ?? [])
          .filter(
            (build) =>
              build.build_number === buildNumber && !recentIds.has(build.id),
          )
          .map((build) => ({
            ...toBuildItem(build, 'open'),
            label: buildLabel(build).replace('Build #', 'Open #'),
            // Always match what was typed, so the result is never filtered out.
            keywords: [search],
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

  const sections = rankPaletteSections(
    [
      { heading: 'Open build', items: openItems },
      { heading: pageCommandsHeading ?? 'This page', items: pageItems },
      { heading: 'Go to', items: navItems },
      { heading: 'Actions', items: actionItems },
      { heading: 'Run pipeline', items: runItems },
      { heading: 'Recent builds', items: buildItems },
      { heading: 'Projects', items: projectItems },
      { heading: 'Settings', items: settingsItems },
      { heading: 'Theme', items: themeItems },
    ].map((section) => ({
      heading: section.heading,
      items: section.items.map((item) => ({
        ...item,
        value: paletteValue(item),
      })),
    })),
    search,
    defaultFilter,
  )

  return (
    <CommandDialog
      open={open}
      onOpenChange={onOpenChange}
      title="Quick open"
      description="Open a page, project, build or action."
    >
      {/* Filtered here: cmdk cannot reorder groups, so the best match could
          sit below a weaker one. */}
      <Command shouldFilter={false}>
        <CommandInput
          aria-label="Find a page, project, build or action"
          value={search}
          onValueChange={setSearch}
          placeholder={
            isQaViewer
              ? 'Find a page or action…'
              : 'Find a project, build #, page or action…'
          }
        />
        <CommandList>
          <CommandEmpty>No results found.</CommandEmpty>
          {sections.map((section, index) => (
            <PaletteGroup
              key={section.heading}
              heading={section.heading}
              items={section.items}
              separated={index > 0 && !search.trim()}
            />
          ))}
        </CommandList>
        {projectStatus ? (
          <Text variant="caption" className="border-t px-3 py-2" role="status">
            {projectStatus}
          </Text>
        ) : null}
      </Command>
    </CommandDialog>
  )
}
