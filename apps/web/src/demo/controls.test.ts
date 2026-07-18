import { beforeEach, describe, expect, it } from 'vitest'

import { queryClient } from '@/lib/query-client'
import { useAuthStore } from '@/stores/auth-store'
import { useRecentProjectsStore } from '@/stores/recent-projects-store'
import { DEMO_INSTANCE_ID } from './seed'
import { activateDemoPersona, demoScenarioUrl } from './controls'

describe('demo controls', () => {
  beforeEach(() => {
    localStorage.clear()
    queryClient.clear()
    useAuthStore.setState({
      instanceId: DEMO_INSTANCE_ID,
      token: null,
      expiresAt: null,
      user: null,
    })
    useRecentProjectsStore.setState({
      projects: [{ id: 'project-1', name: 'Project', visitedAt: 1 }],
    })
  })

  it('switches the real demo auth session and clears role-scoped caches', () => {
    queryClient.setQueryData(['projects'], { projects: ['owner-only'] })

    expect(activateDemoPersona('developer')).toBe(true)

    expect(useAuthStore.getState()).toMatchObject({
      instanceId: DEMO_INSTANCE_ID,
      token: 'demo-session-token-developer',
      user: {
        email: 'demo+developer@oore.build',
        oidc_subject: 'demo::developer',
        role: 'developer',
      },
    })
    expect(queryClient.getQueryCache().getAll()).toHaveLength(0)
    expect(useRecentProjectsStore.getState().projects).toEqual([])
  })

  it('changes scenarios without losing the current route or query', () => {
    expect(
      demoScenarioUrl(
        'https://demo.oore.build/projects/proj-1?page=2&pipelineQ=release#pipelines',
        'degraded',
      ),
    ).toBe(
      'https://demo.oore.build/projects/proj-1?page=2&pipelineQ=release&demoScenario=degraded#pipelines',
    )
  })
})
