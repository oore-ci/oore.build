import { describe, it, expect, vi, beforeEach } from 'vitest'
import {
  ApiClientError,
  getApiErrorMessage,
  getSetupStatus,
  verifyBootstrapToken,
  configureOidc,
  completeSetup,
} from '@/lib/api'

// ── Mock global fetch ──────────────────────────────────────────

const mockFetch = vi.fn()
global.fetch = mockFetch

beforeEach(() => {
  mockFetch.mockReset()
})

// ── ApiClientError ─────────────────────────────────────────────

describe('ApiClientError', () => {
  it('stores status, code, and details', () => {
    const err = new ApiClientError(422, {
      error: 'Validation failed',
      code: 'validation_error',
      details: 'issuer_url is required',
    })

    expect(err).toBeInstanceOf(Error)
    expect(err.name).toBe('ApiClientError')
    expect(err.message).toBe('Validation failed')
    expect(err.status).toBe(422)
    expect(err.code).toBe('validation_error')
    expect(err.details).toBe('issuer_url is required')
  })

  it('handles missing details', () => {
    const err = new ApiClientError(401, {
      error: 'Unauthorized',
      code: 'unauthorized',
    })

    expect(err.details).toBeUndefined()
  })
})

// ── getApiErrorMessage ─────────────────────────────────────────

describe('getApiErrorMessage', () => {
  it('returns mapped message when code is in codeMap', () => {
    const err = new ApiClientError(422, {
      error: 'Server message',
      code: 'invalid_token',
    })
    const result = getApiErrorMessage(err, {
      invalid_token: 'Your token is invalid.',
    })
    expect(result).toBe('Your token is invalid.')
  })

  it('falls back to error.message when code is not in map', () => {
    const err = new ApiClientError(500, {
      error: 'Internal failure',
      code: 'server_error',
    })
    const result = getApiErrorMessage(err, {})
    expect(result).toBe('Internal failure')
  })

  it('returns message for non-ApiClientError Error instances', () => {
    const err = new Error('Network timeout')
    const result = getApiErrorMessage(err, {})
    expect(result).toBe('Network timeout')
  })

  it('returns generic fallback for unknown error types', () => {
    const result = getApiErrorMessage('something weird', {})
    expect(result).toBe('An unexpected error occurred. Please try again.')
  })
})

// ── API functions ──────────────────────────────────────────────

function mockJsonResponse(status: number, body: unknown) {
  return Promise.resolve({
    ok: status >= 200 && status < 300,
    status,
    json: () => Promise.resolve(body),
  })
}

describe('getSetupStatus', () => {
  it('calls GET /v1/public/setup-status', async () => {
    const payload = {
      instance_id: 'test-id',
      state: 'uninitialized',
      setup_mode: true,
      is_configured: false,
    }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    const result = await getSetupStatus()

    expect(mockFetch).toHaveBeenCalledWith('/v1/public/setup-status', {
      headers: { 'Content-Type': 'application/json' },
    })
    expect(result).toEqual(payload)
  })
})

describe('verifyBootstrapToken', () => {
  it('calls POST /v1/setup/bootstrap-token/verify with token', async () => {
    const payload = { session_token: 'sess-abc', expires_at: 9999999 }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    const result = await verifyBootstrapToken('my-token')

    expect(mockFetch).toHaveBeenCalledWith(
      '/v1/setup/bootstrap-token/verify',
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ token: 'my-token' }),
      },
    )
    expect(result).toEqual(payload)
  })
})

describe('configureOidc', () => {
  it('calls POST /v1/setup/oidc/configure with auth header', async () => {
    const payload = {
      state: 'idp_configured',
      discovered_issuer: 'https://issuer.example.com',
    }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    const result = await configureOidc('sess-token', {
      issuer_url: 'https://issuer.example.com',
      client_id: 'cid',
    })

    expect(mockFetch).toHaveBeenCalledWith('/v1/setup/oidc/configure', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: 'Bearer sess-token',
      },
      body: JSON.stringify({
        issuer_url: 'https://issuer.example.com',
        client_id: 'cid',
      }),
    })
    expect(result).toEqual(payload)
  })
})

describe('completeSetup', () => {
  it('calls POST /v1/setup/complete with auth header', async () => {
    const payload = { state: 'ready', instance_id: 'inst-1' }
    mockFetch.mockReturnValue(mockJsonResponse(200, payload))

    const result = await completeSetup('sess-token')

    expect(mockFetch).toHaveBeenCalledWith('/v1/setup/complete', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: 'Bearer sess-token',
      },
    })
    expect(result).toEqual(payload)
  })

  it('throws ApiClientError on non-ok response', async () => {
    mockFetch.mockReturnValue(
      mockJsonResponse(401, {
        error: 'Invalid session',
        code: 'unauthorized',
      }),
    )

    await expect(completeSetup('bad-token')).rejects.toThrow(ApiClientError)
  })
})
