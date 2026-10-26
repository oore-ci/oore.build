import { useMemo, useState } from 'react'

import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
  ComboboxTrigger,
  ComboboxValue,
} from '@/components/ui/combobox'
import { Button } from '@/components/ui/button'
import { DataTableSelectFilter } from '@/components/data-table'
import { useDebouncedCallback } from '@/hooks/use-debounced-callback'
import { useInfinitePipelines, usePipeline } from '@/hooks/use-pipelines'
import { useInfiniteProjects, useProject } from '@/hooks/use-projects'
import { useBuildActors } from '@/hooks/use-builds'
import { BUILD_STATUS_FILTER_OPTIONS } from '@/lib/status-variants'
import { isNearScrollEnd } from '@/lib/scroll'
import { cn } from '@/lib/utils'

export interface BuildFilterValue {
  actor?: string
  branch?: string
  from?: string
  pipeline?: string
  project?: string
  q?: string
  status?: string
  to?: string
  trigger?: BuildTrigger
}

/** Trigger kinds `GET /v1/builds?trigger=` accepts, with their filter labels. */
export const BUILD_TRIGGER_FILTER_OPTIONS = {
  all: 'All triggers',
  push: 'Push',
  pull_request: 'Pull request',
  tag_push: 'Tag push',
  manual: 'Manual',
  schedule: 'Schedule',
} as const

/** Event names providers store for each webhook trigger, such as GitLab's. */
const BUILD_TRIGGER_EVENT_ALIASES = new Map<string, BuildTrigger>([
  ['push', 'push'],
  ['Push Hook', 'push'],
  ['pull_request', 'pull_request'],
  ['Merge Request Hook', 'pull_request'],
  ['tag_push', 'tag_push'],
  ['Tag Push Hook', 'tag_push'],
])

/** The filter a webhook build's stored event belongs to. */
export function webhookEventTrigger(event: string): BuildTrigger | undefined {
  return BUILD_TRIGGER_EVENT_ALIASES.get(event)
}

/** Longest actor `GET /v1/builds` accepts. */
export const MAX_ACTOR_FILTER_LENGTH = 320

export type BuildTrigger = Exclude<
  keyof typeof BUILD_TRIGGER_FILTER_OPTIONS,
  'all'
>

export function isBuildTrigger(value: string): value is BuildTrigger {
  return value !== 'all' && Object.hasOwn(BUILD_TRIGGER_FILTER_OPTIONS, value)
}

export interface BuildFilterControlProps {
  filters: BuildFilterValue
  onChange: (updates: Partial<BuildFilterValue> & { page?: undefined }) => void
}

export function ProjectFilter({
  className,
  filters,
  onChange,
}: Pick<BuildFilterControlProps, 'filters' | 'onChange'> & {
  className?: string
}) {
  const [search, setSearch] = useState('')
  const updateSearch = useDebouncedCallback(
    (value: string) => setSearch(value.trim()),
    300,
  )
  const projectsQuery = useInfiniteProjects({
    limit: 100,
    search: search || undefined,
    sort: 'name',
    direction: 'asc',
  })
  const selectedProjectQuery = useProject(filters.project ?? '')
  const projects = useMemo(
    () => projectsQuery.data?.pages.flatMap((page) => page.projects) ?? [],
    [projectsQuery.data?.pages],
  )
  const selectedProject = filters.project
    ? (projects.find((project) => project.id === filters.project) ??
      selectedProjectQuery.data?.project ??
      null)
    : null
  const options = useMemo(
    () => [
      { id: '', name: 'All projects' },
      ...projects.map(({ id, name }) => ({ id, name })),
    ],
    [projects],
  )
  const selectedOption = selectedProject
    ? { id: selectedProject.id, name: selectedProject.name }
    : options[0]

  return (
    <Combobox
      items={options}
      value={selectedOption}
      disabled={projectsQuery.isLoading}
      filter={null}
      isItemEqualToValue={(item, value) => item.id === value.id}
      itemToStringLabel={(item) => item.name}
      onInputValueChange={(value, details) => {
        if (
          details.reason === 'input-change' ||
          details.reason === 'input-clear'
        ) {
          updateSearch(value)
        }
      }}
      onValueChange={(project) =>
        onChange({
          project: project?.id || undefined,
          // Pipelines belong to one project, so a new project clears it.
          pipeline: undefined,
          page: undefined,
        })
      }
    >
      <ComboboxTrigger
        render={
          <Button
            variant="secondary"
            className={cn('justify-between font-normal', className)}
          />
        }
        aria-label="Filter by project"
      >
        <span className="truncate">
          <ComboboxValue />
        </span>
      </ComboboxTrigger>
      <ComboboxContent>
        <ComboboxInput
          showTrigger={false}
          placeholder="Search projects"
          aria-label="Search projects"
        />
        <ComboboxEmpty>
          {projectsQuery.error
            ? 'Projects could not be loaded.'
            : 'No projects found.'}
        </ComboboxEmpty>
        <ComboboxList
          onScroll={(event) => {
            const list = event.currentTarget
            if (
              isNearScrollEnd(list) &&
              projectsQuery.hasNextPage &&
              !projectsQuery.isFetchingNextPage
            ) {
              void projectsQuery.fetchNextPage()
            }
          }}
        >
          {options.map((project) => (
            <ComboboxItem key={project.id || 'all'} value={project}>
              {project.name}
            </ComboboxItem>
          ))}
        </ComboboxList>
      </ComboboxContent>
    </Combobox>
  )
}

export function PipelineFilter({
  className,
  filters,
  onChange,
  projectId,
}: Pick<BuildFilterControlProps, 'filters' | 'onChange'> & {
  className?: string
  projectId?: string
}) {
  const [search, setSearch] = useState('')
  const updateSearch = useDebouncedCallback(
    (value: string) => setSearch(value.trim()),
    300,
  )
  const selectedPipelineQuery = usePipeline(filters.pipeline ?? '')
  // A pipeline link without a project still lists that pipeline's siblings.
  const scopeProjectId =
    projectId ?? selectedPipelineQuery.data?.pipeline.project_id ?? ''
  const pipelinesQuery = useInfinitePipelines(scopeProjectId, {
    limit: 100,
    search: search || undefined,
    sort: 'name',
    direction: 'asc',
  })
  const pipelines = useMemo(
    () => pipelinesQuery.data?.pages.flatMap((page) => page.pipelines) ?? [],
    [pipelinesQuery.data?.pages],
  )
  const selectedPipeline = filters.pipeline
    ? (pipelines.find((pipeline) => pipeline.id === filters.pipeline) ??
      selectedPipelineQuery.data?.pipeline ??
      null)
    : null
  const options = useMemo(
    () => [
      { id: '', name: 'All pipelines' },
      ...pipelines.map(({ id, name }) => ({ id, name })),
    ],
    [pipelines],
  )
  const selectedOption = selectedPipeline
    ? { id: selectedPipeline.id, name: selectedPipeline.name }
    : options[0]

  return (
    <Combobox
      items={options}
      value={selectedOption}
      disabled={!scopeProjectId || pipelinesQuery.isLoading}
      filter={null}
      isItemEqualToValue={(item, value) => item.id === value.id}
      itemToStringLabel={(item) => item.name}
      onInputValueChange={(value, details) => {
        if (
          details.reason === 'input-change' ||
          details.reason === 'input-clear'
        ) {
          updateSearch(value)
        }
      }}
      onValueChange={(pipeline) =>
        onChange({
          pipeline: pipeline?.id || undefined,
          page: undefined,
        })
      }
    >
      <ComboboxTrigger
        render={
          <Button
            variant="secondary"
            className={cn('justify-between font-normal', className)}
          />
        }
        aria-label="Filter by pipeline"
      >
        <span className="truncate">
          <ComboboxValue />
        </span>
      </ComboboxTrigger>
      <ComboboxContent>
        <ComboboxInput
          showTrigger={false}
          placeholder="Search pipelines"
          aria-label="Search pipelines"
        />
        <ComboboxEmpty>
          {pipelinesQuery.error
            ? 'Pipelines could not be loaded.'
            : 'No pipelines found.'}
        </ComboboxEmpty>
        <ComboboxList
          onScroll={(event) => {
            const list = event.currentTarget
            if (
              isNearScrollEnd(list) &&
              pipelinesQuery.hasNextPage &&
              !pipelinesQuery.isFetchingNextPage
            ) {
              void pipelinesQuery.fetchNextPage()
            }
          }}
        >
          {options.map((pipeline) => (
            <ComboboxItem key={pipeline.id || 'all'} value={pipeline}>
              {pipeline.name}
            </ComboboxItem>
          ))}
        </ComboboxList>
      </ComboboxContent>
    </Combobox>
  )
}

export function StatusFilter({
  filters,
  onChange,
}: Pick<BuildFilterControlProps, 'filters' | 'onChange'>) {
  return (
    <DataTableSelectFilter
      value={filters.status ?? 'all'}
      onValueChange={(value) =>
        onChange({
          status: value && value !== 'all' ? value : undefined,
          page: undefined,
        })
      }
      options={BUILD_STATUS_FILTER_OPTIONS}
    />
  )
}

export function TriggerFilter({
  filters,
  onChange,
}: Pick<BuildFilterControlProps, 'filters' | 'onChange'>) {
  return (
    <DataTableSelectFilter
      value={filters.trigger ?? 'all'}
      onValueChange={(value) =>
        onChange({
          trigger: isBuildTrigger(value) ? value : undefined,
          page: undefined,
        })
      }
      options={BUILD_TRIGGER_FILTER_OPTIONS}
    />
  )
}

/**
 * A combobox over free-form values such as actors or branches. Typing offers
 * the typed text itself, so a value missing from the suggestions still works.
 */
function ValueFilter({
  allLabel,
  className,
  emptyMessage,
  label,
  loading,
  onSearch,
  onValueChange,
  placeholder,
  suggestions,
  value,
}: {
  allLabel: string
  className?: string
  emptyMessage: string
  label: string
  loading?: boolean
  onSearch?: (value: string) => void
  onValueChange: (value: string | undefined) => void
  placeholder: string
  suggestions: ReadonlyArray<string>
  value?: string
}) {
  const [typed, setTyped] = useState('')
  const needle = typed.trim()
  const options = useMemo(() => {
    const matching = suggestions.filter((suggestion) =>
      suggestion.toLowerCase().includes(needle.toLowerCase()),
    )
    const values =
      needle && !matching.includes(needle) ? [needle, ...matching] : matching
    return [
      { id: '', name: allLabel },
      ...values.map((id) => ({ id, name: id })),
    ]
  }, [allLabel, needle, suggestions])
  const selected = value ? { id: value, name: value } : options[0]

  return (
    <Combobox
      items={options}
      value={selected}
      filter={null}
      isItemEqualToValue={(item, other) => item.id === other.id}
      itemToStringLabel={(item) => item.name}
      onInputValueChange={(next, details) => {
        if (
          details.reason === 'input-change' ||
          details.reason === 'input-clear'
        ) {
          setTyped(next)
          onSearch?.(next.trim())
        }
      }}
      onOpenChange={(open) => {
        if (!open) {
          setTyped('')
          onSearch?.('')
        }
      }}
      onValueChange={(option) => onValueChange(option?.id || undefined)}
    >
      <ComboboxTrigger
        render={
          <Button
            variant="secondary"
            className={cn('justify-between font-normal', className)}
          />
        }
        aria-label={label}
      >
        <span className="truncate">
          <ComboboxValue />
        </span>
      </ComboboxTrigger>
      {/* Emails and branch names run longer than the trigger. */}
      <ComboboxContent className="w-72">
        <ComboboxInput
          showTrigger={false}
          placeholder={placeholder}
          aria-label={placeholder}
        />
        <ComboboxEmpty>{loading ? 'Loading…' : emptyMessage}</ComboboxEmpty>
        <ComboboxList>
          {options.map((option) => (
            <ComboboxItem key={option.id || 'all'} value={option}>
              <span className="truncate">{option.name}</span>
            </ComboboxItem>
          ))}
        </ComboboxList>
      </ComboboxContent>
    </Combobox>
  )
}

export function ActorFilter({
  className,
  filters,
  onChange,
}: Pick<BuildFilterControlProps, 'filters' | 'onChange'> & {
  className?: string
}) {
  const [search, setSearch] = useState('')
  const updateSearch = useDebouncedCallback(setSearch, 300)
  const actorsQuery = useBuildActors({
    project_id: filters.project,
    q: search || undefined,
  })

  return (
    <ValueFilter
      allLabel="Anyone"
      className={className}
      emptyMessage={
        actorsQuery.error ? 'Actors could not be loaded.' : 'No actors found.'
      }
      label="Filter by who triggered the build"
      loading={actorsQuery.isLoading}
      onSearch={updateSearch}
      onValueChange={(actor) => onChange({ actor, page: undefined })}
      placeholder="Search actors"
      suggestions={actorsQuery.data?.actors ?? []}
      value={filters.actor}
    />
  )
}

export function BranchFilter({
  branches,
  className,
  filters,
  onChange,
}: Pick<BuildFilterControlProps, 'filters' | 'onChange'> & {
  /** Branches worth suggesting, such as those on the loaded page. */
  branches: ReadonlyArray<string>
  className?: string
}) {
  return (
    <ValueFilter
      allLabel="All branches"
      className={className}
      emptyMessage="Type a branch name."
      label="Filter by branch"
      onValueChange={(branch) => onChange({ branch, page: undefined })}
      placeholder="Branch name"
      suggestions={branches}
      value={filters.branch}
    />
  )
}
