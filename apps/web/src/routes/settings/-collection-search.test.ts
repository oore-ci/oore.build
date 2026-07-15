import { describe, expect, it } from 'vitest'

import { parseApiTokensSearch } from './api-tokens'
import { parseRunnersSearch } from './runners'

describe('settings collection search state', () => {
  it('keeps valid API token collection controls', () => {
    expect(
      parseApiTokensSearch({
        q: '  deploy  ',
        sort: 'last_used_at',
        direction: 'asc',
        page: '3',
        pageSize: '50',
      }),
    ).toEqual({
      q: 'deploy',
      sort: 'last_used_at',
      direction: 'asc',
      page: 3,
      pageSize: 50,
    })
  })

  it('drops invalid runner collection controls to route defaults', () => {
    expect(
      parseRunnersSearch({
        q: ' ',
        sort: 'version',
        direction: 'sideways',
        page: '-1',
        pageSize: '25',
      }),
    ).toEqual({
      q: undefined,
      sort: undefined,
      direction: undefined,
      page: undefined,
      pageSize: undefined,
    })
  })
})
