import { describe, expect, it } from 'vitest'

import { resolveBreadcrumbPath } from './breadcrumbs'

describe('resolveBreadcrumbPath', () => {
  it('fills dynamic parent paths and refuses broken links', () => {
    expect(
      resolveBreadcrumbPath('/projects/$projectId', {
        projectId: 'kite mobile',
      }),
    ).toBe('/projects/kite%20mobile')
    expect(resolveBreadcrumbPath('/projects/$projectId', {})).toBeUndefined()
  })
})
