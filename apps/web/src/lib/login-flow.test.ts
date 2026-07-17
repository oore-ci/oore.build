import { describe, expect, it } from 'vitest'
import { resolveLoginFlow } from '@/lib/login-flow'

describe('resolveLoginFlow', () => {
  it('uses trusted proxy flow for remote trusted-proxy mode', () => {
    const flow = resolveLoginFlow(
      {
        runtime_mode: 'remote',
        remote_auth_mode: 'trusted_proxy',
      },
      false,
    )

    expect(flow).toBe('trusted_proxy')
  })

  it('uses oidc flow for remote oidc mode', () => {
    const flow = resolveLoginFlow(
      {
        runtime_mode: 'remote',
        remote_auth_mode: 'oidc',
      },
      false,
    )

    expect(flow).toBe('oidc')
  })

  it('uses recovery before the configured remote auth flow', () => {
    const flow = resolveLoginFlow(
      {
        runtime_mode: 'remote',
        remote_auth_mode: 'trusted_proxy',
      },
      true,
    )

    expect(flow).toBe('recovery')
  })

  it('uses local flow for local mode', () => {
    const flow = resolveLoginFlow(
      {
        runtime_mode: 'local',
        remote_auth_mode: 'oidc',
      },
      false,
    )

    expect(flow).toBe('local')
  })

  it('uses recovery before remote oidc', () => {
    const flow = resolveLoginFlow(
      {
        runtime_mode: 'remote',
        remote_auth_mode: 'oidc',
      },
      true,
    )

    expect(flow).toBe('recovery')
  })
})
