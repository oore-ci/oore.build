import { HttpResponse, delay } from 'msw'
import * as z from 'zod'

import type {
  PipelineWorkflowSettings,
  SigningIdentity,
  SigningIdentityProfile,
} from '@oore/client/models'
import { SIGNING_IDENTITY_NAME } from '@/lib/signing-identities'
import { requireDemoProjectPermission } from '../authorization'
import { getDemoPersonaFromRequest, getDemoProjectRole } from '../personas'
import { ago } from '../seed'
import { demoState } from '../state'
import { demoApi } from './api'

const DEMO_TEAM_ID = 'A1B2C3D4E5'

const androidSchema = z.object({
  keystore_filename: z.string().trim().min(1),
  keystore_base64: z.string().trim().min(1),
  store_password: z.string().trim().min(1),
  key_alias: z.string().trim().min(1),
  key_password: z.string().trim().min(1),
})
const iosSchema = z.object({
  p12_filename: z.string().trim().min(1),
  p12_base64: z.string().trim().min(1),
  p12_password: z.string(),
  profiles: z
    .array(
      z.object({
        profile_filename: z.string().trim().min(1),
        profile_base64: z.string().trim().min(1),
      }),
    )
    .min(1, 'ios.profiles needs at least one provisioning profile'),
})
const createSchema = z.object({
  name: z.string(),
  kind: z.enum(['android_keystore', 'ios_distribution']),
  android: androidSchema.nullish(),
  ios: iosSchema.nullish(),
})
const updateSchema = z.object({
  name: z.string().nullish(),
  android: androidSchema.nullish(),
  ios: iosSchema.nullish(),
})
const settingsSchema = z.object({
  android_identity_id: z.string().nullish(),
  ios_identity_id: z.string().nullish(),
  publish_enabled: z.boolean().nullish(),
  approval_required: z.boolean().nullish(),
})

function failure(message: string, status = 400, code = 'invalid_input') {
  return HttpResponse.json({ error: message, code }, { status })
}

function withUses(identity: SigningIdentity): SigningIdentity {
  const used_by = demoState.pipelines
    .filter((pipeline) => pipeline.project_id === identity.project_id)
    .flatMap((pipeline) =>
      (demoState.workflowSettings[pipeline.id] ?? [])
        .filter(
          (entry) =>
            entry.android_identity_id === identity.id ||
            entry.ios_identity_id === identity.id,
        )
        .map((entry) => ({
          pipeline_id: pipeline.id,
          pipeline_name: pipeline.name,
          workflow: entry.workflow,
        })),
    )
  return { ...identity, used_by }
}

function normalizeName(name: string) {
  const trimmed = name.trim()
  return SIGNING_IDENTITY_NAME.test(trimmed) ? trimmed : undefined
}

/**
 * The demo cannot open certificates or profiles, so it reads what the
 * daemon would from the file names: "adhoc" or "ad-hoc" marks ad hoc
 * profiles, and a name like `com.acme.coin.mobileprovision` is the bundle.
 */
function summarize(
  kind: SigningIdentity['kind'],
  android?: z.infer<typeof androidSchema> | null,
  ios?: z.infer<typeof iosSchema> | null,
): { error: string } | { summary: Partial<SigningIdentity> } {
  if (kind === 'android_keystore') {
    if (!android || ios) {
      return {
        error: 'an android_keystore identity takes `android` and no `ios`',
      }
    }
    return {
      summary: {
        filename: android.keystore_filename,
        key_alias: android.key_alias,
      },
    }
  }
  if (!ios || android) {
    return {
      error: 'an ios_distribution identity takes `ios` and no `android`',
    }
  }
  const adHoc = ios.profiles.map((profile) =>
    /ad[-_ ]?hoc/i.test(profile.profile_filename),
  )
  if (adHoc.some(Boolean) && !adHoc.every(Boolean)) {
    return {
      error:
        'ios.profiles mixes ad hoc and App Store profiles; one identity exports one way',
    }
  }
  const profiles: Array<SigningIdentityProfile> = ios.profiles.map(
    (profile) => {
      const stem = profile.profile_filename.replace(/\.mobileprovision$/i, '')
      return {
        bundle_id: stem.includes('.') ? stem : `com.acme.${stem}`,
        profile_name: stem,
        expires_at: ago(-365 * 86_400),
      }
    },
  )
  return {
    summary: {
      filename: ios.p12_filename,
      team_id: DEMO_TEAM_ID,
      export_method: adHoc[0] ? 'ad_hoc' : 'app_store',
      expires_at: ago(-365 * 86_400),
      profiles,
    },
  }
}

function findIdentity(projectId: string, id: string) {
  return demoState.signingIdentities.find(
    (identity) => identity.project_id === projectId && identity.id === id,
  )
}

function nameTaken(projectId: string, name: string, except?: string) {
  return demoState.signingIdentities.some(
    (identity) =>
      identity.project_id === projectId &&
      identity.name === name &&
      identity.id !== except,
  )
}

export const signingHandlers = [
  demoApi.listSigningIdentities(async ({ params, request }) => {
    await delay(150)
    const projectId = String(params.project_id)
    const persona = getDemoPersonaFromRequest(request)
    if (!getDemoProjectRole(persona, projectId)) {
      return failure('Project not found', 404, 'not_found')
    }
    return HttpResponse.json({
      identities: demoState.signingIdentities
        .filter((identity) => identity.project_id === projectId)
        .sort(
          (a, b) =>
            a.kind.localeCompare(b.kind) || a.name.localeCompare(b.name),
        )
        .map(withUses),
    })
  }),

  demoApi.createSigningIdentity(async ({ params, request }) => {
    await delay(400)
    const projectId = String(params.project_id)
    const forbidden = requireDemoProjectPermission(
      request,
      projectId,
      'pipelines:write',
    )
    if (forbidden) return forbidden
    const parsed = createSchema.safeParse(await request.json())
    if (!parsed.success) return failure(parsed.error.issues[0].message)
    const name = normalizeName(parsed.data.name)
    if (!name) {
      return failure(
        "name must start with a letter or digit and use only letters, digits, '-' and '_' (max 64 chars)",
      )
    }
    if (nameTaken(projectId, name)) {
      return failure(
        `This project already has a signing identity named ${name}`,
        409,
        'name_taken',
      )
    }
    const summarized = summarize(
      parsed.data.kind,
      parsed.data.android,
      parsed.data.ios,
    )
    if ('error' in summarized) return failure(summarized.error)
    const identity: SigningIdentity = {
      id: `sid-demo-${crypto.randomUUID().slice(0, 8)}`,
      project_id: projectId,
      name,
      kind: parsed.data.kind,
      ...summarized.summary,
      created_at: ago(0),
      updated_at: ago(0),
    }
    demoState.signingIdentities.push(identity)
    return HttpResponse.json({ identity: withUses(identity) }, { status: 201 })
  }),

  demoApi.updateSigningIdentity(async ({ params, request }) => {
    await delay(350)
    const projectId = String(params.project_id)
    const forbidden = requireDemoProjectPermission(
      request,
      projectId,
      'pipelines:write',
    )
    if (forbidden) return forbidden
    const identity = findIdentity(projectId, String(params.identity_id))
    if (!identity)
      return failure('Signing identity not found', 404, 'not_found')
    const parsed = updateSchema.safeParse(await request.json())
    if (!parsed.success) return failure(parsed.error.issues[0].message)
    let name = identity.name
    if (parsed.data.name != null) {
      const normalized = normalizeName(parsed.data.name)
      if (!normalized) {
        return failure(
          "name must start with a letter or digit and use only letters, digits, '-' and '_' (max 64 chars)",
        )
      }
      if (nameTaken(projectId, normalized, identity.id)) {
        return failure(
          `This project already has a signing identity named ${normalized}`,
          409,
          'name_taken',
        )
      }
      name = normalized
    }
    if (parsed.data.android || parsed.data.ios) {
      const summarized = summarize(
        identity.kind,
        parsed.data.android,
        parsed.data.ios,
      )
      if ('error' in summarized) return failure(summarized.error)
      Object.assign(identity, {
        filename: undefined,
        key_alias: undefined,
        team_id: undefined,
        export_method: undefined,
        expires_at: undefined,
        profiles: undefined,
        ...summarized.summary,
      })
    }
    identity.name = name
    identity.updated_at = ago(0)
    return HttpResponse.json({ identity: withUses(identity) })
  }),

  demoApi.deleteSigningIdentity(async ({ params, request }) => {
    await delay(300)
    const projectId = String(params.project_id)
    const forbidden = requireDemoProjectPermission(
      request,
      projectId,
      'pipelines:write',
    )
    if (forbidden) return forbidden
    const identity = findIdentity(projectId, String(params.identity_id))
    if (!identity)
      return failure('Signing identity not found', 404, 'not_found')
    const uses = withUses(identity).used_by ?? []
    if (uses.length > 0) {
      const list = uses
        .map((use) => `${use.pipeline_name} (${use.workflow})`)
        .join(', ')
      return failure(
        `Workflow settings use this identity: ${list}. Pick another there first.`,
        409,
        'identity_in_use',
      )
    }
    demoState.signingIdentities = demoState.signingIdentities.filter(
      (candidate) => candidate.id !== identity.id,
    )
    return HttpResponse.json({ ok: true })
  }),

  demoApi.listPipelineWorkflowSettings(async ({ params, request }) => {
    await delay(150)
    const pipeline = demoState.pipelines.find(
      (candidate) => candidate.id === params.pipeline_id,
    )
    const persona = getDemoPersonaFromRequest(request)
    if (!pipeline || !getDemoProjectRole(persona, pipeline.project_id)) {
      return failure('Pipeline not found', 404, 'not_found')
    }
    return HttpResponse.json({
      pipeline_id: pipeline.id,
      settings: [...(demoState.workflowSettings[pipeline.id] ?? [])].sort(
        (a, b) => a.workflow.localeCompare(b.workflow),
      ),
    })
  }),

  demoApi.updatePipelineWorkflowSettings(async ({ params, request }) => {
    await delay(300)
    const pipeline = demoState.pipelines.find(
      (candidate) => candidate.id === params.pipeline_id,
    )
    if (!pipeline) return failure('Pipeline not found', 404, 'not_found')
    const forbidden = requireDemoProjectPermission(
      request,
      pipeline.project_id,
      'pipelines:write',
    )
    if (forbidden) return forbidden
    const workflow = String(params.workflow)
    if (!SIGNING_IDENTITY_NAME.test(workflow)) {
      return failure('workflow is not a valid workflow name')
    }
    const parsed = settingsSchema.safeParse(await request.json())
    if (!parsed.success) return failure(parsed.error.issues[0].message)
    const body = parsed.data
    const list = demoState.workflowSettings[pipeline.id] ?? []
    const existing = list.find((entry) => entry.workflow === workflow)
    if (
      (existing?.approval_required ?? null) !== (body.approval_required ?? null)
    ) {
      const cannotApprove = requireDemoProjectPermission(
        request,
        pipeline.project_id,
        'builds:approve',
      )
      if (cannotApprove) return cannotApprove
    }
    for (const [field, kind] of [
      ['android_identity_id', 'android_keystore'],
      ['ios_identity_id', 'ios_distribution'],
    ] as const) {
      const id = body[field]
      if (!id) continue
      const identity = findIdentity(pipeline.project_id, id)
      if (!identity) {
        return failure(`${field} is not a signing identity of this project`)
      }
      if (identity.kind !== kind) {
        return failure(`${field} must be an ${kind} identity`)
      }
    }
    const rest = list.filter((entry) => entry.workflow !== workflow)
    const empty =
      !body.android_identity_id &&
      !body.ios_identity_id &&
      body.publish_enabled == null &&
      body.approval_required == null
    if (empty) {
      demoState.workflowSettings[pipeline.id] = rest
      return HttpResponse.json({ workflow })
    }
    const saved: PipelineWorkflowSettings = {
      workflow,
      android_identity_id: body.android_identity_id ?? undefined,
      ios_identity_id: body.ios_identity_id ?? undefined,
      publish_enabled: body.publish_enabled ?? undefined,
      approval_required: body.approval_required ?? undefined,
      updated_at: ago(0),
    }
    demoState.workflowSettings[pipeline.id] = [...rest, saved]
    return HttpResponse.json(saved)
  }),
]
