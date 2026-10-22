import {
  PipelineFilter,
  ProjectFilter,
  StatusFilter,
  type BuildFilterValue,
} from './-build-filter-controls'

interface BuildFiltersProps {
  filters: BuildFilterValue
  onChange: (updates: Partial<BuildFilterValue> & { page?: undefined }) => void
}

export function BuildFilters({ filters, onChange }: BuildFiltersProps) {
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
    </>
  )
}
