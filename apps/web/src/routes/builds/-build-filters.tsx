import {
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
      <StatusFilter filters={filters} onChange={onChange} />
    </>
  )
}
