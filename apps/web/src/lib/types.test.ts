import { describe, it, expect } from 'vitest'
import type {
  SetupState,
  SetupStatus,
  BootstrapTokenVerifyResponse,
  OidcConfigureRequest,
  OidcConfigureResponse,
  SetupOidcStartResponse,
  SetupOidcVerifyResponse,
  SetupCompleteResponse,
  ApiError,
} from '@/lib/types'

describe('types', () => {
  it('SetupState accepts valid states', () => {
    const states: SetupState[] = [
      'uninitialized',
      'bootstrap_pending',
      'idp_configured',
      'owner_created',
      'ready',
    ]
    expect(states).toHaveLength(5)
  })

  it('SetupStatus can be constructed', () => {
    const status: SetupStatus = {
      instance_id: 'test',
      state: 'uninitialized',
      setup_mode: true,
      is_configured: false,
    }
    expect(status.instance_id).toBe('test')
  })

  it('BootstrapTokenVerifyResponse can be constructed', () => {
    const resp: BootstrapTokenVerifyResponse = {
      session_token: 'tok',
      expires_at: 123,
    }
    expect(resp.session_token).toBe('tok')
  })

  it('OidcConfigureRequest can be constructed', () => {
    const req: OidcConfigureRequest = {
      issuer_url: 'https://example.com',
      client_id: 'cid',
    }
    expect(req.issuer_url).toBe('https://example.com')
  })

  it('OidcConfigureResponse can be constructed', () => {
    const resp: OidcConfigureResponse = {
      state: 'idp_configured',
      discovered_issuer: 'https://example.com',
    }
    expect(resp.state).toBe('idp_configured')
  })

  it('SetupOidcStartResponse can be constructed', () => {
    const resp: SetupOidcStartResponse = {
      authorization_url: 'https://auth.example.com',
      state: 'random-state',
    }
    expect(resp.authorization_url).toBe('https://auth.example.com')
  })

  it('SetupOidcVerifyResponse can be constructed', () => {
    const resp: SetupOidcVerifyResponse = {
      state: 'owner_created',
      owner_email: 'user@example.com',
      oidc_subject: 'sub-123',
    }
    expect(resp.state).toBe('owner_created')
  })

  it('SetupCompleteResponse can be constructed', () => {
    const resp: SetupCompleteResponse = {
      state: 'ready',
      instance_id: 'inst-1',
    }
    expect(resp.state).toBe('ready')
  })

  it('ApiError can be constructed', () => {
    const err: ApiError = {
      error: 'Something went wrong',
      code: 'internal_error',
      details: 'stack trace here',
    }
    expect(err.code).toBe('internal_error')
  })
})
