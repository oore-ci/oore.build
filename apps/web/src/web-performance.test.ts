import { describe, expect, it } from 'vitest'

import { webPerformancePersona, webReleaseChannel } from './web-performance'

describe('web performance labels', () => {
  it('reduces routes and roles to fixed privacy-safe personas', () => {
    expect(
      webPerformancePersona(
        '/builds/private-build-id',
        'developer',
        false,
        false,
      ),
    ).toBe('operator_build_detail')
    expect(
      webPerformancePersona(
        '/builds/private-build-id',
        'qa_viewer',
        true,
        false,
      ),
    ).toBe('qa_install')
    expect(webPerformancePersona('/settings/users', 'owner', true, false)).toBe(
      'admin',
    )
    expect(
      webPerformancePersona('/projects/private-id', 'developer', true, false),
    ).toBe('mobile_shell')
  })

  it('accepts only release channels with bounded cardinality', () => {
    expect(webReleaseChannel('alpha')).toBe('alpha')
    expect(webReleaseChannel('customer-123')).toBe('dev')
    expect(webReleaseChannel(undefined)).toBe('dev')
  })
})
