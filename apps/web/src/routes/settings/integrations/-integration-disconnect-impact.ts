import type { ListProjectsResponse, Project } from '@/lib/types'

const PROJECT_PAGE_SIZE = 200

export async function loadAffectedProjects(
  repositoryIds: ReadonlySet<string>,
  loadPage: (offset: number, limit: number) => Promise<ListProjectsResponse>,
): Promise<Array<Project>> {
  const affected: Array<Project> = []
  let offset = 0
  let total = Number.POSITIVE_INFINITY

  while (offset < total) {
    const page = await loadPage(offset, PROJECT_PAGE_SIZE)
    total = page.total
    affected.push(
      ...page.projects.filter(
        (project) =>
          project.repository_id && repositoryIds.has(project.repository_id),
      ),
    )

    if (page.projects.length === 0) break
    offset += page.projects.length
  }

  return affected.sort((left, right) => left.name.localeCompare(right.name))
}
