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
import { BUILD_STATUS_FILTER_OPTIONS } from '@/lib/status-variants'
import { isNearScrollEnd } from '@/lib/scroll'
import { cn } from '@/lib/utils'

export interface BuildFilterValue {
  pipeline?: string
  project?: string
  q?: string
  status?: string
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
