import { describe, expect, it } from 'vitest'

import { needsDirectRunnerPolicySetup } from './direct-runner-policy-banner'

describe('direct runner policy banner', () => {
  it('stays visible until the instance and every known repository are approved', () => {
    expect(
      needsDirectRunnerPolicySetup({
        instanceEnabled: false,
        repositoriesKnown: true,
        unapprovedRepositoryCount: 0,
      }),
    ).toBe(true)
    expect(
      needsDirectRunnerPolicySetup({
        instanceEnabled: true,
        repositoriesKnown: true,
        unapprovedRepositoryCount: 1,
      }),
    ).toBe(true)
    expect(
      needsDirectRunnerPolicySetup({
        instanceEnabled: true,
        repositoriesKnown: false,
        unapprovedRepositoryCount: 0,
      }),
    ).toBe(true)
    expect(
      needsDirectRunnerPolicySetup({
        instanceEnabled: true,
        repositoriesKnown: true,
        unapprovedRepositoryCount: 0,
      }),
    ).toBe(false)
  })
})
