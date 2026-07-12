import { describe, expect, it } from 'vitest'

import { gitLabPublicEndpoints, normalizeGitLabHostUrl } from './gitlab-url'

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

describe('gitLabPublicEndpoints', () => {
  it('uses the configured public frontend instead of a private browser route', () => {
    expect(
      gitLabPublicEndpoints(
        'https://oore.example.com/',
        'http://100.107.193.1:8787',
      ),
    ).toEqual({
      callbackUrl: 'https://oore.example.com/v1/integrations/gitlab/callback',
      webhookUrl: 'https://oore.example.com/v1/webhooks/gitlab',
    })
  })

  it('falls back to the current frontend origin', () => {
    expect(gitLabPublicEndpoints(undefined, 'https://ci.example.com')).toEqual({
      callbackUrl: 'https://ci.example.com/v1/integrations/gitlab/callback',
      webhookUrl: 'https://ci.example.com/v1/webhooks/gitlab',
    })
  })
})
