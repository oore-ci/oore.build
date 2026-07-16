export const PROJECT_BUILD_SORT_OPTIONS = {
  created_at: 'Newest first',
  status: 'Status',
  pipeline_name: 'Pipeline',
  branch: 'Branch',
} as const

export type ProjectBuildSort = keyof typeof PROJECT_BUILD_SORT_OPTIONS
