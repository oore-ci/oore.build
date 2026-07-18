import { describe, expect, it, vi } from 'vitest'

import type { Project } from '@/lib/types'
import { loadAffectedProjects } from './-integration-disconnect-impact'

function project(index: number, repositoryId?: string): Project {
  return {
    id: `project-${index}`,
    name: `Project ${String(index).padStart(3, '0')}`,
    repository_id: repositoryId,
    settings: {},
    created_by: 'owner-1',
    created_at: index,
    updated_at: index,
  }
}

describe('integration disconnect impact', () => {
  it('walks every project page before deriving affected projects', async () => {
    const projects = Array.from({ length: 431 }, (_, index) =>
      project(index + 1, index % 137 === 0 ? 'repository-affected' : undefined),
    )
    const loadPage = vi.fn((offset: number, limit: number) =>
      Promise.resolve({
        projects: projects.slice(offset, offset + limit),
        total: projects.length,
      }),
    )

    const affected = await loadAffectedProjects(
      new Set(['repository-affected']),
      loadPage,
    )

    expect(loadPage.mock.calls).toEqual([
      [0, 200],
      [200, 200],
      [400, 200],
    ])
    expect(affected.map((item) => item.id)).toEqual([
      'project-1',
      'project-138',
      'project-275',
      'project-412',
    ])
  })

  it('stops safely when an API page is unexpectedly empty', async () => {
    const loadPage = vi.fn().mockResolvedValue({ projects: [], total: 20 })

    await expect(
      loadAffectedProjects(new Set(['repository-1']), loadPage),
    ).resolves.toEqual([])
    expect(loadPage).toHaveBeenCalledOnce()
  })
})
