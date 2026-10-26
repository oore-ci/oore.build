import {
  DateRangeFilter,
  searchDateRange,
} from '@/components/date-range-filter'
import {
  ActorFilter,
  BranchFilter,
  PipelineFilter,
  ProjectFilter,
  StatusFilter,
  TriggerFilter,
  type BuildFilterValue,
} from './-build-filter-controls'

interface BuildFiltersProps {
  branches: ReadonlyArray<string>
  filters: BuildFilterValue
  onChange: (updates: Partial<BuildFilterValue> & { page?: undefined }) => void
}

export function BuildFilters({
  branches,
  filters,
  onChange,
}: BuildFiltersProps) {
  return (
    <>
      <ProjectFilter className="w-44" filters={filters} onChange={onChange} />
      {/* Pipelines are listed per project, so pick a project first. */}
      {filters.project || filters.pipeline ? (
        <PipelineFilter
          className="w-44"
          filters={filters}
          onChange={onChange}
          projectId={filters.project}
        />
      ) : null}
      <StatusFilter filters={filters} onChange={onChange} />
      <TriggerFilter filters={filters} onChange={onChange} />
      <BranchFilter
        branches={branches}
        className="w-40"
        filters={filters}
        onChange={onChange}
      />
      <ActorFilter className="w-44" filters={filters} onChange={onChange} />
      <DateRangeFilter
        from={filters.from}
        to={filters.to}
        placeholder="Any time"
        onChange={(range) =>
          onChange({ ...searchDateRange(range), page: undefined })
        }
      />
    </>
  )
}
