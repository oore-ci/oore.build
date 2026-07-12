import { describe, expect, it } from 'vitest'

import { normalizeGitLabHostUrl } from './gitlab-url'

describe('normalizeGitLabHostUrl', () => {
  it('keeps only a normalized HTTP(S) host origin', () => {
    expect(normalizeGitLabHostUrl(' https://gitlab.example.test/ ')).toBe(
      'https://gitlab.example.test',
    )
    expect(normalizeGitLabHostUrl('http://gitlab.internal:8443')).toBe(
      'http://gitlab.internal:8443',
    )
  })

  it('rejects credentials and non-host URLs', () => {
    expect(normalizeGitLabHostUrl('gitlab.example.test')).toBeNull()
    expect(
      normalizeGitLabHostUrl('https://user@gitlab.example.test'),
    ).toBeNull()
    expect(
      normalizeGitLabHostUrl('https://gitlab.example.test/api/v4'),
    ).toBeNull()
  })
})
