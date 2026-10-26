import { HttpResponse, delay } from 'msw'
import * as z from 'zod'

import type { AppStoreConnectKeySummary } from '@oore/client/models'
import { requireDemoInstancePermission } from '../authorization'
import { ago } from '../seed'
import { demoApi } from './api'

export const DEMO_SHARED_KEY_ID = 'asc-key-example-team'

interface DemoKey {
  id: string
  name: string
  key_id: string
  issuer_id: string
  created_at: number
  updated_at: number
}

const demoKeys: Array<DemoKey> = [
  {
    id: DEMO_SHARED_KEY_ID,
    name: 'Example team key',
    key_id: 'T8K2L4M6NP',
    issuer_id: '69d2de96-0000-47e3-e053-0824d011072a',
    created_at: ago(40 * 86_400),
    updated_at: ago(40 * 86_400),
  },
]

/** Pipelines whose iOS signing uses each shared key, set by the pipelines handlers. */
let countUsage: (keyId: string) => number = () => 0

export function setDemoSharedKeyUsage(counter: (keyId: string) => number) {
  countUsage = counter
}

export function demoSharedKeySummary(
  id: string,
): AppStoreConnectKeySummary | undefined {
  const key = demoKeys.find((candidate) => candidate.id === id)
  return key ? { ...key, pipeline_count: countUsage(id) } : undefined
}

const identifier = z
  .string()
  .trim()
  .regex(/^[A-Za-z0-9-]+$/)
const createSchema = z.object({
  name: z.string().trim().min(1).max(64),
  key_id: identifier,
  issuer_id: identifier,
  private_key_base64: z.string().min(1),
})
const updateSchema = createSchema.partial()

function invalid(message: string, status = 400, code = 'invalid_input') {
  return HttpResponse.json({ error: message, code }, { status })
}

export const storeCredentialHandlers = [
  demoApi.listAppStoreConnectKeys(async () => {
    await delay(120)
    return HttpResponse.json({
      keys: demoKeys.map((key) => demoSharedKeySummary(key.id)!),
    })
  }),

  demoApi.createAppStoreConnectKey(async ({ request }) => {
    await delay(250)
    const forbidden = requireDemoInstancePermission(
      request,
      'instance_settings:write',
    )
    if (forbidden) return forbidden
    const parsed = createSchema.safeParse(await request.json())
    if (!parsed.success) return invalid('Check the Key ID, Issuer ID and .p8')
    if (demoKeys.some((key) => key.name === parsed.data.name)) {
      return invalid(
        `A shared key named '${parsed.data.name}' already exists`,
        409,
        'key_exists',
      )
    }
    const key: DemoKey = {
      id: crypto.randomUUID(),
      name: parsed.data.name,
      key_id: parsed.data.key_id,
      issuer_id: parsed.data.issuer_id,
      created_at: ago(0),
      updated_at: ago(0),
    }
    demoKeys.push(key)
    return HttpResponse.json(
      { key: demoSharedKeySummary(key.id) },
      { status: 201 },
    )
  }),

  demoApi.updateAppStoreConnectKey(async ({ params, request }) => {
    await delay(250)
    const forbidden = requireDemoInstancePermission(
      request,
      'instance_settings:write',
    )
    if (forbidden) return forbidden
    const key = demoKeys.find((candidate) => candidate.id === params.id)
    if (!key) return invalid('Shared key not found', 404, 'not_found')
    const parsed = updateSchema.safeParse(await request.json())
    if (!parsed.success) return invalid('Check the Key ID, Issuer ID and .p8')
    key.name = parsed.data.name ?? key.name
    key.key_id = parsed.data.key_id ?? key.key_id
    key.issuer_id = parsed.data.issuer_id ?? key.issuer_id
    key.updated_at = ago(0)
    return HttpResponse.json({ key: demoSharedKeySummary(key.id) })
  }),

  demoApi.deleteAppStoreConnectKey(async ({ params, request }) => {
    await delay(200)
    const forbidden = requireDemoInstancePermission(
      request,
      'instance_settings:write',
    )
    if (forbidden) return forbidden
    const index = demoKeys.findIndex((candidate) => candidate.id === params.id)
    if (index < 0) return invalid('Shared key not found', 404, 'not_found')
    const usage = countUsage(demoKeys[index].id)
    if (usage > 0) {
      return invalid(
        `${usage === 1 ? '1 pipeline uses' : `${usage} pipelines use`} this key. Switch their iOS signing to another key before deleting it.`,
        409,
        'key_in_use',
      )
    }
    demoKeys.splice(index, 1)
    return HttpResponse.json({ ok: true })
  }),
]
