import { describe, expect, it } from 'vitest'

import { canAccessSettings, settingsGroupsForRole } from './settings-navigation'

function titlesFor(role: 'owner' | 'admin' | 'developer' | 'qa_viewer') {
  return settingsGroupsForRole(role).flatMap((group) =>
    group.items.map((item) => item.title),
  )
}

describe('settings navigation', () => {
  it('keeps the full grouped hub available to instance administrators', () => {
    expect(titlesFor('owner')).toEqual([
      'General',
      'Runners',
      'Sources',
      'Artifact storage',
      'Retention',
      'Users',
      'API tokens',
      'Notifications',
      'Audit log',
    ])
  })

  it('limits developers to their supported read and token surfaces', () => {
    expect(titlesFor('developer')).toEqual([
      'Runners',
      'Sources',
      'API tokens',
    ])
  })

  it('keeps the tester workspace out of instance settings', () => {
    expect(canAccessSettings('qa_viewer')).toBe(false)
    expect(settingsGroupsForRole('qa_viewer')).toEqual([])
  })
})
