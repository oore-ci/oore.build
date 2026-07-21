import type { BuildStatus, Build } from '@/lib/types'

const ACTIVE_BUILD_STATUSES = new Set<BuildStatus>([
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
