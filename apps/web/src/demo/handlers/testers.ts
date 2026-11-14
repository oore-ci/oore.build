import { HttpResponse, delay } from 'msw'
import * as z from 'zod'
import type {
  ArtifactInstall,
  BuildRetention,
  BuildShare,
  DeviceRegistration,
  LatestInstallLink,
  RecentFeedback,
  TesterGroup,
  TesterGroupMember,
} from '@oore/client/models'

import { demoApi } from './api'
import { requireDemoProjectPermission } from '../authorization'
import { getDemoPersonaFromRequest, getDemoProjectRole } from '../personas'
import { BUILD_IDS, USER_IDS, ago } from '../seed'
import { demoState } from '../state'

const DAY = 86_400

const groupSchema = z.object({
  name: z.string(),
  member_user_ids: z.array(z.string()),
})
const sendSchema = z.object({
  group_id: z.string(),
  message: z.string().nullish(),
})
const linkSchema = z.object({
  platform: z.enum(['android', 'ios', 'macos']),
  branch: z.string().nullish(),
})
const registrationSchema = z.object({ device_name: z.string().nullish() })

function member(userId: string): TesterGroupMember | null {
  const user = demoState.users.find((candidate) => candidate.id === userId)
  return user
    ? { user_id: user.id, email: user.email, name: user.display_name }
    : null
}

function members(userIds: Array<string>): Array<TesterGroupMember> {
  return userIds.flatMap((id) => member(id) ?? [])
}

const groups = new Map<string, TesterGroup>([
  [
    'tg-demo-qa',
    {
      id: 'tg-demo-qa',
      name: 'QA team',
      members: members([USER_IDS.qaViewer, USER_IDS.developer, USER_IDS.admin]),
      created_at: ago(DAY * 20),
      updated_at: ago(DAY * 3),
    },
  ],
  [
    'tg-demo-design',
    {
      id: 'tg-demo-design',
      name: 'Design review',
      members: members([USER_IDS.owner, USER_IDS.qaViewer]),
      created_at: ago(DAY * 12),
      updated_at: ago(DAY * 12),
    },
  ],
])

const sharesByBuild = new Map<string, Array<BuildShare>>([
  [
    BUILD_IDS.succeeded1,
    [
      {
        id: 'share-demo-001',
        build_id: BUILD_IDS.succeeded1,
        group_id: 'tg-demo-qa',
        group_name: 'QA team',
        message: 'Check the new checkout and saved addresses.',
        sent_by_email: 'demo+developer@oore.build',
        sent_at: ago(7200),
        recipients: members([
          USER_IDS.qaViewer,
          USER_IDS.developer,
          USER_IDS.admin,
        ]).map((recipient, index) => ({
          ...recipient,
          installed_at: index < 2 ? ago(6000 - index * 1500) : null,
          no_access: false,
        })),
      },
    ],
  ],
])

const linksByPipeline = new Map<string, Array<LatestInstallLink>>()
const keptBuilds = new Set<string>()
const registrations = new Map<
  string,
  { registration: DeviceRegistration; polls: number }
>()

function seededLinks(pipelineId: string): Array<LatestInstallLink> {
  const existing = linksByPipeline.get(pipelineId)
  if (existing) return existing
  const pipeline = demoState.pipelines.find((p) => p.id === pipelineId)
  const platforms = (pipeline?.execution_config.platforms ?? []).filter(
    (platform) => platform !== 'macos',
  )
  const links: Array<LatestInstallLink> = platforms
    .slice(0, 1)
    .map((platform) => ({
      id: `latest-demo-${pipelineId}-${platform}`,
      pipeline_id: pipelineId,
      project_id: pipeline?.project_id ?? '',
      platform,
      branch: 'main',
      prefix: 'ol_7Hq2',
      created_at: ago(DAY * 9),
      created_by_email: 'demo+developer@oore.build',
      last_used_at: ago(3 * 3600),
      use_count: 14,
    }))
  linksByPipeline.set(pipelineId, links)
  return links
}

function installsFor(buildId: string): Array<ArtifactInstall> {
  const artifact = demoState.artifacts[buildId]?.[0]
  return (sharesByBuild.get(buildId) ?? [])
    .flatMap((share) => share.recipients)
    .flatMap((recipient) =>
      recipient.installed_at
        ? [
            {
              artifact_id: artifact?.id ?? '',
              user_email: recipient.email,
              via: 'install_page',
              installed_at: recipient.installed_at,
            },
          ]
        : [],
    )
    .sort((a, b) => b.installed_at - a.installed_at)
}

/** What retention will do with a build, for the build page. */
export function demoRetention(buildId: string): BuildRetention {
  const live = demoState.releases.some(
    (release) =>
      release.build_id === buildId &&
      ['rolling', 'paused', 'in_review', 'complete'].includes(release.state),
  )
  const build = demoState.builds.find((candidate) => candidate.id === buildId)
  return {
    kept: keptBuilds.has(buildId),
    kept_by_release: live,
    kept_by_status: false,
    expires_at:
      keptBuilds.has(buildId) || live
        ? null
        : (build?.created_at ?? ago(0)) + 30 * DAY,
  }
}

function notFound(what: string) {
  return HttpResponse.json(
    { error: `${what} not found`, code: 'not_found' },
    { status: 404 },
  )
}

function visibleBuild(request: Request, buildId: string) {
  const build = demoState.builds.find((b) => b.id === buildId)
  if (!build) return null
  const persona = getDemoPersonaFromRequest(request)
  return getDemoProjectRole(persona, build.project_id) ? build : null
}

export const testerHandlers = [
  demoApi.listTesterGroups(async () => {
    await delay(120)
    return HttpResponse.json({ groups: [...groups.values()] })
  }),

  demoApi.createTesterGroup(async ({ request }) => {
    const body = groupSchema.parse(await request.json())
    const group: TesterGroup = {
      id: `tg-demo-${crypto.randomUUID().slice(0, 8)}`,
      name: body.name,
      members: members(body.member_user_ids),
      created_at: ago(0),
      updated_at: ago(0),
    }
    groups.set(group.id, group)
    return HttpResponse.json(group)
  }),

  demoApi.updateTesterGroup(async ({ params, request }) => {
    const existing = groups.get(String(params.group_id))
    if (!existing) return notFound('Group')
    const body = groupSchema.parse(await request.json())
    const group = {
      ...existing,
      name: body.name,
      members: members(body.member_user_ids),
      updated_at: ago(0),
    }
    groups.set(group.id, group)
    return HttpResponse.json(group)
  }),

  demoApi.deleteTesterGroup(({ params }) => {
    groups.delete(String(params.group_id))
    return new HttpResponse(null, { status: 204 })
  }),

  demoApi.listBuildShares(async ({ params, request }) => {
    await delay(120)
    const build = visibleBuild(request, String(params.build_id))
    if (!build) return notFound('Build')
    return HttpResponse.json({ shares: sharesByBuild.get(build.id) ?? [] })
  }),

  demoApi.sendBuildToGroup(async ({ params, request }) => {
    const build = visibleBuild(request, String(params.build_id))
    if (!build) return notFound('Build')
    const forbidden = requireDemoProjectPermission(
      request,
      build.project_id,
      'artifacts:write',
    )
    if (forbidden) return forbidden
    const body = sendSchema.parse(await request.json())
    const group = groups.get(body.group_id)
    if (!group) return notFound('Group')
    const share: BuildShare = {
      id: `share-demo-${crypto.randomUUID().slice(0, 8)}`,
      build_id: build.id,
      group_id: group.id,
      group_name: group.name,
      message: body.message ?? null,
      sent_by_email: getDemoPersonaFromRequest(request).email,
      sent_at: ago(0),
      recipients: group.members.map((recipient) => ({
        ...recipient,
        installed_at: null,
        no_access: false,
      })),
    }
    sharesByBuild.set(build.id, [share, ...(sharesByBuild.get(build.id) ?? [])])
    return HttpResponse.json(share)
  }),

  demoApi.listBuildInstalls(async ({ params, request }) => {
    await delay(120)
    const build = visibleBuild(request, String(params.build_id))
    if (!build) return notFound('Build')
    return HttpResponse.json({ installs: installsFor(build.id) })
  }),

  demoApi.keepBuild(({ params }) => {
    keptBuilds.add(String(params.build_id))
    return new HttpResponse(null, { status: 204 })
  }),

  demoApi.unkeepBuild(({ params }) => {
    keptBuilds.delete(String(params.build_id))
    return new HttpResponse(null, { status: 204 })
  }),

  demoApi.listLatestInstallLinks(async ({ params }) => {
    await delay(120)
    return HttpResponse.json({ links: seededLinks(String(params.pipeline_id)) })
  }),

  demoApi.createLatestInstallLink(async ({ params, request }) => {
    const pipelineId = String(params.pipeline_id)
    const body = linkSchema.parse(await request.json())
    const pipeline = demoState.pipelines.find((p) => p.id === pipelineId)
    const token = crypto.randomUUID().replaceAll('-', '')
    const link: LatestInstallLink = {
      id: `latest-demo-${token.slice(0, 8)}`,
      pipeline_id: pipelineId,
      project_id: pipeline?.project_id ?? '',
      platform: body.platform,
      branch: body.branch ?? null,
      prefix: `ol_${token.slice(0, 4)}`,
      url: `${globalThis.location.origin}/install/latest/ol_${token}`,
      created_at: ago(0),
      created_by_email: getDemoPersonaFromRequest(request).email,
      last_used_at: null,
      use_count: 0,
    }
    linksByPipeline.set(pipelineId, [
      ...seededLinks(pipelineId),
      { ...link, url: null },
    ])
    return HttpResponse.json(link)
  }),

  demoApi.deleteLatestInstallLink(({ params }) => {
    for (const [pipelineId, links] of linksByPipeline) {
      linksByPipeline.set(
        pipelineId,
        links.filter((link) => link.id !== params.link_id),
      )
    }
    return new HttpResponse(null, { status: 204 })
  }),

  demoApi.listRecentFeedback(async ({ request }) => {
    await delay(150)
    const days = Number(new URL(request.url).searchParams.get('days') ?? 7)
    const since = ago(days * DAY)
    const feedback: Array<RecentFeedback> = []
    for (const [buildId, notes] of Object.entries(demoState.buildFeedback)) {
      const build = visibleBuild(request, buildId)
      if (!build) continue
      const project = demoState.projects.find((p) => p.id === build.project_id)
      for (const note of notes ?? []) {
        if (note.created_at < since) continue
        feedback.push({
          feedback: note,
          project_id: build.project_id,
          project_name: project?.name ?? 'App',
          build_number: build.build_number,
        })
      }
    }
    feedback.sort((a, b) => b.feedback.created_at - a.feedback.created_at)
    return HttpResponse.json({ feedback })
  }),

  demoApi.createDeviceRegistration(async ({ params, request }) => {
    await delay(300)
    const body = registrationSchema.parse(await request.json())
    const id = `devreg-demo-${crypto.randomUUID().slice(0, 8)}`
    const registration: DeviceRegistration = {
      id,
      pipeline_id: String(params.pipeline_id),
      state: 'pending',
      device_name: body.device_name ?? null,
      profile_url: `${globalThis.location.origin}/install/register-device/demo/profile.mobileconfig`,
      created_at: ago(0),
      expires_at: ago(-15 * 60),
    }
    registrations.set(id, { registration, polls: 0 })
    return HttpResponse.json(registration)
  }),

  demoApi.getDeviceRegistration(({ params }) => {
    const entry = registrations.get(String(params.registration_id))
    if (!entry) return notFound('Registration')
    entry.polls += 1
    // The demo phone "installs" the profile after a few seconds.
    if (entry.polls > 2) {
      entry.registration = {
        ...entry.registration,
        state: 'registered',
        device_name: entry.registration.device_name ?? 'Taylor’s iPhone',
        profile_url: null,
      }
    }
    return HttpResponse.json(entry.registration)
  }),

  demoApi.getWebPushConfig(() =>
    HttpResponse.json({
      public_key:
        'BEl62iUYgUivxIkv69yViEuiBIa-Ib9-SkvMeAtA3LFgDzkrxZJjSgSnfckjBJuBkr3qBUYIHBQFLXYp5Nksh8U',
      subscribed: false,
    }),
  ),
]
