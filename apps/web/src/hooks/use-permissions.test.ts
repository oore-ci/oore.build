import { describe, expect, it } from 'vitest'
import { hasProjectPermission } from './use-permissions'

describe('project permissions', () => {
  it('lets maintainers administer projects', () => {
    expect(hasProjectPermission('maintainer', 'projects', 'write')).toBe(true)
    expect(hasProjectPermission('maintainer', 'projects', 'delete')).toBe(true)
  })

  it('lets developers operate pipelines and builds but not project settings', () => {
    expect(hasProjectPermission('developer', 'pipelines', 'write')).toBe(true)
    expect(hasProjectPermission('developer', 'builds', 'write')).toBe(true)
    expect(hasProjectPermission('developer', 'projects', 'write')).toBe(false)
  })

  it('keeps viewers read-only', () => {
    expect(hasProjectPermission('viewer', 'builds', 'read')).toBe(true)
    expect(hasProjectPermission('viewer', 'builds', 'write')).toBe(false)
    expect(hasProjectPermission(undefined, 'projects', 'read')).toBe(false)
  })
})
