import type { Build, Project } from '@/lib/types'

const ACTIVE_BUILD_STATUSES = new Set([
  'queued',
  'scheduled',
  'assigned',
  'running',
])

export function selectDashboardBuilds(builds: Array<Build>): {
  active: Array<Build>
  recentCompleted: Array<Build>
} {
  return {
    active: builds.filter((build) => ACTIVE_BUILD_STATUSES.has(build.status)),
    recentCompleted: builds
      .filter((build) => !ACTIVE_BUILD_STATUSES.has(build.status))
      .slice(0, 6),
  }
}

export function selectDashboardProjects(
  projects: Array<Project>,
): Array<Project> {
  return projects.slice(0, 6)
}
