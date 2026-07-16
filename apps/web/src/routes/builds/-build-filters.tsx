import { Button } from '@/components/ui/button'
import { CollectionSearchInput } from '@/components/collection-search-input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import type { SortDirection } from '@/components/collection-controls'
import type { Project } from '@/lib/types'
import { BUILD_STATUS_FILTER_OPTIONS } from '@/lib/status-variants'
import { cn } from '@/lib/utils'
import type { BuildSort } from './-build-inventory'

interface BuildFilterValue {
  project?: string
  q?: string
  status?: string
}

interface BuildFiltersProps {
  direction: SortDirection
  filters: BuildFilterValue
  onChange: (updates: Partial<BuildFilterValue> & { page?: undefined }) => void
  onSortChange: (sort: BuildSort, direction: SortDirection) => void
  projects: Array<Project>
  projectsResolved: boolean
  sort: BuildSort
}

const BUILD_SORT_OPTIONS: Record<BuildSort, string> = {
  created_at: 'Newest first',
  status: 'Status',
  project_name: 'Project',
  pipeline_name: 'Pipeline',
  branch: 'Branch',
}

export function BuildFilters({
  direction,
  filters,
  onChange,
  onSortChange,
  projects,
  projectsResolved,
  sort,
}: BuildFiltersProps) {
  const hasFilters = !!filters.q || !!filters.project || !!filters.status
  const clearFilters = () =>
    onChange({
      q: undefined,
      project: undefined,
      status: undefined,
      page: undefined,
    })

  return (
    <div className="flex flex-col gap-3 lg:flex-row lg:items-center">
      <CollectionSearchInput
        key={filters.q ?? ''}
        initialValue={filters.q ?? ''}
        onSearch={(value) =>
          onChange({ q: value.trim() || undefined, page: undefined })
        }
        placeholder="Search by branch"
        ariaLabel="Search builds by branch"
      />
      <div className="grid grid-cols-2 gap-3 sm:flex sm:flex-wrap lg:ml-auto">
        <Button
          variant="ghost"
          size="sm"
          className={cn('hidden sm:inline-flex', !hasFilters && 'invisible')}
          aria-hidden={!hasFilters}
          tabIndex={hasFilters ? undefined : -1}
          onClick={clearFilters}
        >
          Clear filters
        </Button>
        <Select
          value={filters.project ?? 'all'}
          onValueChange={(value) =>
            onChange({
              project: value && value !== 'all' ? value : undefined,
              page: undefined,
            })
          }
          items={Object.fromEntries([
            ['all', 'All projects'],
            ...projects.map((project) => [project.id, project.name] as const),
          ])}
          disabled={!projectsResolved}
        >
          <SelectTrigger
            className="w-full sm:w-44"
            aria-label="Filter by project"
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">All projects</SelectItem>
            {projects.map((project) => (
              <SelectItem key={project.id} value={project.id}>
                {project.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Select
          value={filters.status ?? 'all'}
          onValueChange={(value) =>
            onChange({
              status: value && value !== 'all' ? value : undefined,
              page: undefined,
            })
          }
          items={BUILD_STATUS_FILTER_OPTIONS}
        >
          <SelectTrigger
            className="w-full sm:w-40"
            aria-label="Filter by status"
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {Object.entries(BUILD_STATUS_FILTER_OPTIONS).map(
              ([value, label]) => (
                <SelectItem key={value} value={value}>
                  {label}
                </SelectItem>
              ),
            )}
          </SelectContent>
        </Select>
        <Select
          value={sort}
          onValueChange={(value) =>
            onSortChange(value ?? 'created_at', direction)
          }
          items={BUILD_SORT_OPTIONS}
        >
          <SelectTrigger
            className="col-span-2 w-full sm:w-40 lg:hidden"
            aria-label="Sort builds"
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {Object.entries(BUILD_SORT_OPTIONS).map(([value, label]) => (
              <SelectItem key={value} value={value}>
                {label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        {hasFilters ? (
          <Button
            variant="ghost"
            size="sm"
            className="col-span-2 sm:hidden"
            onClick={clearFilters}
          >
            Clear filters
          </Button>
        ) : null}
      </div>
    </div>
  )
}
