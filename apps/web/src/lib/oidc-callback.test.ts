import { describe, expect, it } from 'vitest'
import { precheckOidcCallback } from '@/lib/oidc-callback'

describe('precheckOidcCallback', () => {
  it('maps provider error to a failed precheck', () => {
    const params = new URLSearchParams(
      'error=invalid_client&error_description=Bad+client+credentials',
    )
    const result = precheckOidcCallback(params, 'state-1', 'setup_owner')
    expect(result.ok).toBe(false)
    expect(result.target).toBe('/setup/owner')
    expect(result.hint).toBe('invalid_client')
  })

  it('fails when callback params are missing', () => {
    const params = new URLSearchParams('')
    const result = precheckOidcCallback(params, 'state-1', 'auth')
    expect(result.ok).toBe(false)
    expect(result.hint).toBe('missing_callback_params')
    expect(result.target).toBe('/login')
  })

  it('fails when stored state is missing', () => {
    const params = new URLSearchParams('code=abc&state=state-1')
    const result = precheckOidcCallback(params, null, 'auth')
    expect(result.ok).toBe(false)
    expect(result.hint).toBe('missing_stored_state')
  })

  it('fails when state does not match', () => {
    const params = new URLSearchParams('code=abc&state=state-2')
    const result = precheckOidcCallback(params, 'state-1', 'setup_owner')
    expect(result.ok).toBe(false)
    expect(result.hint).toBe('state_mismatch')
    expect(result.target).toBe('/setup/owner')
  })

  it('passes and returns normalized flow payload', () => {
    const params = new URLSearchParams('code=abc&state=state-1')
    const result = precheckOidcCallback(params, 'state-1', 'setup_owner')
    expect(result).toEqual({
      ok: true,
      target: '/setup/owner',
      flow: 'setup_owner',
      code: 'abc',
      state: 'state-1',
    })
  })
})
