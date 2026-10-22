import { describe, expect, test } from 'bun:test'

import {
  compareOperations,
  sdkOperations,
  specOperations,
} from './check-client-openapi'

const spec = {
  paths: {
    '/healthz': { get: {} },
    '/v1/builds/{build_id}': { get: {}, delete: {}, parameters: [] },
    '/v1/events': { get: {} },
  },
}

const sdk = `
export const healthz = (options) => (options?.client ?? client).get({
    responseStyle: 'data',
    url: '/healthz',
    ...options
});
export const deleteBuild = (options) => (options.client ?? client).delete({
    responseStyle: 'data',
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/v1/builds/{build_id}',
    ...options
});
export const streamEvents = (options) => (options?.client ?? client).sse.get({
    security: [{ scheme: 'bearer', type: 'http' }],
    url: '/v1/events',
    ...options
});
`

describe('check-client-openapi', () => {
  test('reads operations from the spec and the generated SDK', () => {
    expect([...specOperations(spec)].sort()).toEqual([
      'DELETE /v1/builds/{build_id}',
      'GET /healthz',
      'GET /v1/builds/{build_id}',
      'GET /v1/events',
    ])
    expect([...sdkOperations(sdk)].sort()).toEqual([
      'DELETE /v1/builds/{build_id}',
      'GET /healthz',
      'GET /v1/events',
    ])
  })

  test('reports operations missing from either side', () => {
    expect(
      compareOperations(
        specOperations(spec),
        new Set([...sdkOperations(sdk), 'POST /v1/removed']),
      ),
    ).toEqual({
      missing: ['GET /v1/builds/{build_id}'],
      extra: ['POST /v1/removed'],
    })
    expect(
      compareOperations(specOperations(spec), specOperations(spec)),
    ).toEqual({ missing: [], extra: [] })
  })
})
