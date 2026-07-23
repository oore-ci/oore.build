import { describe, expect, it } from 'vitest'
import { hasInstancePermission, hasProjectPermission } from './use-permissions'

describe('instance permissions', () => {
  it('blocks developer project creation and keeps inventory read-only', () => {
    expect(hasInstancePermission('developer', 'projects', 'write')).toBe(false)
    expect(hasInstancePermission('developer', 'runners', 'read')).toBe(true)
    expect(hasInstancePermission('developer', 'runners', 'write')).toBe(false)
    expect(hasInstancePermission('developer', 'integrations', 'read')).toBe(
      true,
    )
    expect(hasInstancePermission('developer', 'integrations', 'write')).toBe(
      false,
    )
  })
})

describe('project permissions', () => {
  it('lets maintainers administer projects', () => {
    expect(hasProjectPermission('maintainer', 'projects', 'write')).toBe(true)
    expect(hasProjectPermission('maintainer', 'projects', 'delete')).toBe(true)
    expect(hasProjectPermission('maintainer', 'pipelines', 'delete')).toBe(true)
  })

  it('lets developers operate pipelines and builds but not project settings', () => {
    expect(hasProjectPermission('developer', 'pipelines', 'write')).toBe(true)
    expect(hasProjectPermission('developer', 'builds', 'write')).toBe(true)
    expect(hasProjectPermission('developer', 'projects', 'write')).toBe(false)
  })

  it('keeps viewers read-only', () => {
    expect(hasProjectPermission('viewer', 'builds', 'read')).toBe(true)
    expect(hasProjectPermission('viewer', 'builds', 'write')).toBe(false)
    expect(hasProjectPermission('viewer', 'projects', 'delete')).toBe(false)
    expect(hasProjectPermission('viewer', 'pipelines', 'delete')).toBe(false)
    expect(hasProjectPermission(undefined, 'projects', 'read')).toBe(false)
  })
})
