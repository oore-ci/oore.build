import { describe, expect, it } from 'vitest'

import type { Build, Project } from '@/lib/types'
import { selectDashboardBuilds, selectDashboardProjects } from './dashboard'

function build(id: string, status: Build['status']): Build {
  return {
    id,
    project_id: 'project-1',
    pipeline_id: 'pipeline-1',
    build_number: Number(id.replace(/\D/g, '')) || 1,
    status,
    trigger_type: 'manual',
    config_snapshot: {},
    queued_at: 1,
    created_at: 1,
    updated_at: 1,
  }
}

describe('dashboard recency', () => {
  it('keeps all active builds separate and caps completed builds at six', () => {
    const completed = Array.from({ length: 12 }, (_, index) =>
      build(`completed-${index}`, 'succeeded'),
    )
    const active = [build('active-1', 'queued'), build('active-2', 'running')]

    expect(selectDashboardBuilds([...active, ...completed])).toMatchObject({
      active,
      recentCompleted: completed.slice(0, 6),
    })
  })

  it('caps recent projects at six', () => {
    const projects = Array.from(
      { length: 10 },
      (_, index) =>
        ({
          id: `project-${index}`,
          name: `Project ${index}`,
          settings: {},
          created_by: 'owner-1',
          created_at: index,
          updated_at: index,
        }) satisfies Project,
    )

    expect(selectDashboardProjects(projects)).toEqual(projects.slice(0, 6))
  })
})
