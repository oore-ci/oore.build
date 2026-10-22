import { isOoreApiError } from '@oore/client/client'
import { useProject, useProjects } from '@/hooks/use-projects'
import { useQaSelectedProject } from '@/stores/qa-releases-store'

/**
 * The app the QA release hub shows: the one this viewer picked on this
 * instance, or the first accessible app when nothing was picked or the saved
 * app was deleted or is no longer shared with them.
 */
export function useQaProjectSelection() {
  const { selectedProjectId: storedProjectId, setSelectedProjectId } =
    useQaSelectedProject()
  const storedProjectQuery = useProject(storedProjectId ?? '')
  const storedProjectGone =
    isOoreApiError(storedProjectQuery.error) &&
    (storedProjectQuery.error.status === 403 ||
      storedProjectQuery.error.status === 404)
  const usesStoredProject = storedProjectId != null && !storedProjectGone
  const firstProjectQuery = useProjects({
    limit: 1,
    sort: 'name',
    direction: 'asc',
  })
  const project = usesStoredProject
    ? storedProjectQuery.data?.project
    : firstProjectQuery.data?.projects.at(0)

  return {
    project,
    projectCount: firstProjectQuery.data?.total ?? 0,
    isLoading:
      firstProjectQuery.isLoading ||
      (usesStoredProject && storedProjectQuery.isLoading),
    error:
      firstProjectQuery.error ??
      (usesStoredProject ? storedProjectQuery.error : null),
    refetch: () => {
      void firstProjectQuery.refetch()
      if (usesStoredProject) void storedProjectQuery.refetch()
    },
    setSelectedProjectId,
  }
}
