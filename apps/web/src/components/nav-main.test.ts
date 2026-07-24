import { describe, expect, it } from 'vitest'

import { isSidebarItemActive, sidebarGroupsForRole } from './nav-main'

function navigationFor(role: 'owner' | 'admin' | 'developer' | 'qa_viewer') {
  return sidebarGroupsForRole(role).map((group) => ({
    title: group.title,
    items: group.items.map((item) => ({
      title: item.title,
      to: item.to,
    })),
  }))
}

describe('sidebar navigation', () => {
  it('shows administrators the complete flattened route hierarchy', () => {
    const expected = [
      {
        title: 'Workspace',
        items: [
          { title: 'Dashboard', to: '/' },
          { title: 'Projects', to: '/projects' },
          { title: 'Builds', to: '/builds' },
        ],
      },
      {
        title: 'Settings',
        items: [
          { title: 'Overview', to: '/settings' },
          { title: 'General', to: '/settings/preferences' },
          { title: 'Runners', to: '/settings/runners' },
          { title: 'Sources', to: '/settings/integrations' },
          { title: 'Artifact storage', to: '/settings/artifacts' },
          { title: 'Retention', to: '/settings/retention' },
          { title: 'Users', to: '/settings/users' },
          { title: 'API tokens', to: '/settings/api-tokens' },
          { title: 'Notifications', to: '/settings/notifications' },
          { title: 'Audit log', to: '/settings/audit-log' },
        ],
      },
    ]

    expect(navigationFor('owner')).toEqual(expected)
    expect(navigationFor('admin')).toEqual(expected)
  })

  it('shows developers only routes allowed by the settings guards', () => {
    expect(navigationFor('developer')).toEqual([
      {
        title: 'Workspace',
        items: [
          { title: 'Dashboard', to: '/' },
          { title: 'Projects', to: '/projects' },
          { title: 'Builds', to: '/builds' },
        ],
      },
      {
        title: 'Settings',
        items: [
          { title: 'Overview', to: '/settings' },
          { title: 'Runners', to: '/settings/runners' },
          { title: 'Sources', to: '/settings/integrations' },
          { title: 'API tokens', to: '/settings/api-tokens' },
        ],
      },
    ])
  })

  it('keeps the QA release workspace outside the operator sidebar', () => {
    expect(navigationFor('qa_viewer')).toEqual([])
  })

  it('activates exact overview routes and the owning entry for deep routes', () => {
    expect(isSidebarItemActive('/', '/')).toBe(true)
    expect(isSidebarItemActive('/projects/project-1', '/projects')).toBe(true)
    expect(isSidebarItemActive('/builds/build-1', '/builds')).toBe(true)
    expect(isSidebarItemActive('/settings', '/settings')).toBe(true)
    expect(
      isSidebarItemActive(
        '/settings/integrations/integration-1',
        '/settings/integrations',
      ),
    ).toBe(true)
    expect(
      isSidebarItemActive(
        '/settings/notifications/channel-1',
        '/settings/notifications',
      ),
    ).toBe(true)
    expect(
      isSidebarItemActive('/settings/integrations/integration-1', '/settings'),
    ).toBe(false)
    expect(isSidebarItemActive('/projects-old', '/projects')).toBe(false)
  })
})
