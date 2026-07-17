import { describe, expect, it } from 'vitest'

import {
  DIRECT_RUNNER_TRUST_NOTICE_VERSION,
  directRunnerTrustNoticeKey,
  shouldShowDirectRunnerTrustNotice,
} from './direct-runner-policy-banner-utils'

describe('direct runner trust notice', () => {
  it('keys acknowledgement by notice version, instance, and user', () => {
    expect(directRunnerTrustNoticeKey('instance-1', 'user-1')).toBe(
      `${DIRECT_RUNNER_TRUST_NOTICE_VERSION}:instance-1:user-1`,
    )
  })

  it('shows once to owners and admins', () => {
    const key = directRunnerTrustNoticeKey('instance-1', 'user-1')

    expect(shouldShowDirectRunnerTrustNotice('owner', key, {})).toBe(true)
    expect(shouldShowDirectRunnerTrustNotice('admin', key, {})).toBe(true)
    expect(
      shouldShowDirectRunnerTrustNotice('owner', key, { [key]: true }),
    ).toBe(false)
  })

  it('does not share acknowledgement across versions, instances, or users', () => {
    const key = directRunnerTrustNoticeKey('instance-1', 'user-1')
    const acknowledgements: Record<string, true> = {
      'direct-runner-protocol-3:instance-1:user-1': true,
      [directRunnerTrustNoticeKey('instance-2', 'user-1')]: true,
      [directRunnerTrustNoticeKey('instance-1', 'user-2')]: true,
    }

    expect(
      shouldShowDirectRunnerTrustNotice('owner', key, acknowledgements),
    ).toBe(true)
  })

  it('never shows to roles that cannot configure runner policy', () => {
    const key = directRunnerTrustNoticeKey('instance-1', 'user-1')

    expect(shouldShowDirectRunnerTrustNotice('developer', key, {})).toBe(false)
    expect(shouldShowDirectRunnerTrustNotice('viewer', key, {})).toBe(false)
    expect(shouldShowDirectRunnerTrustNotice(undefined, key, {})).toBe(false)
  })
})
