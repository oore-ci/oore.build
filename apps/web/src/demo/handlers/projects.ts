import { demoApi } from './api'
import { HttpResponse, delay } from 'msw'
import * as z from 'zod'
import { ago } from '../seed'
import { getDemoPersonaFromRequest, getDemoProjectRole } from '../personas'
import {
  requireDemoInstancePermission,
  requireDemoProjectPermission,
} from '../authorization'
import { demoState } from '../state'
import { demoAppStorePublishEnabled } from './pipelines'
import { demoPlayPublishEnabled } from './secrets'
import { parseDemoJsonObject } from '../request'
import type {
  AddProjectMemberRequest,
  BuildPlatform,
  CreateProjectRequest,
  UpdateProjectMemberRequest,
} from '@oore/client/models'
import type { DemoProject } from '../state'

const projectRoleSchema = z.enum(['maintainer', 'developer', 'viewer'])
const projectMemberRequestSchema = z.toZod<AddProjectMemberRequest>()(
  z.object({
    user_id: z.string(),
    role: projectRoleSchema,
  }),
)
const projectRoleRequestSchema = z.toZod<UpdateProjectMemberRequest>()(
  z.object({ role: projectRoleSchema }),
)
const createProjectRequestSchema = z.toZod<CreateProjectRequest>()(
  z.object({
    name: z.string(),
    description: z.string().nullish(),
    repository_id: z.string().nullish(),
    local_repository_path: z.string().nullish(),
    default_branch: z.string().nullish(),
    auto_add_pipelines: z.boolean().nullish(),
  }),
)

const signingEnabledSchema = z.object({ enabled: z.boolean() })

/** What the project has set up after its first install. */
function demoSetupProgress(projectId: string) {
  const pipelineIds = demoState.pipelines
    .filter((pipeline) => pipeline.project_id === projectId)
    .map((pipeline) => pipeline.id)
  return {
    release_signing: pipelineIds.some(
      (id) =>
        signingEnabledSchema.safeParse(demoState.androidSigning[id]?.release)
          .data?.enabled === true ||
        signingEnabledSchema.safeParse(demoState.iosSigning[id]).data
          ?.enabled === true,
    ),
    store_publishing: pipelineIds.some(
      (id) => demoPlayPublishEnabled(id) || demoAppStorePublishEnabled(id),
    ),
    testers: false,
  }
}

const LIVE_STATES = new Set(['rolling', 'complete'])

/** Adds the per-person and derived fields the backend computes. */
function decorateProject<T extends DemoProject>(project: T, userId: string) {
  const platforms = new Set<BuildPlatform>()
  for (const pipeline of demoState.pipelines) {
    if (pipeline.project_id !== project.id) continue
    for (const platform of pipeline.execution_config.platforms) {
      platforms.add(platform)
    }
  }
  const lastBuild = demoState.builds
    .filter((build) => build.project_id === project.id)
    .reduce<number | undefined>(
      (latest, build) => Math.max(latest ?? 0, build.created_at),
      undefined,
    )
  const live = demoState.releases
    .filter(
      (release) =>
        release.project_id === project.id &&
        LIVE_STATES.has(release.state) &&
        release.version_name,
    )
    .sort((left, right) => right.updated_at - left.updated_at)[0]
  return {
    ...project,
    is_favorite: (demoState.favoriteProjects[userId] ?? []).includes(
      project.id,
    ),
    platforms: (['android', 'ios', 'macos'] as const).filter((platform) =>
      platforms.has(platform),
    ),
    last_build_at: lastBuild,
    live_version: live?.version_name ?? undefined,
  }
}

function hasProjectMembership(projectId: string, userId: string): boolean {
  return !!demoState.projectRoles[projectId]?.[userId]
}

function hasRepository(repositoryId: string): boolean {
  return Object.values(demoState.repositories).some((repositories) =>
    repositories?.some((repository) => repository.id === repositoryId),
  )
}

export const projectHandlers = [
  demoApi.listProjects(async ({ request }) => {
    await delay(150)
    const url = new URL(request.url)
    const persona = getDemoPersonaFromRequest(request)
    let projects = demoState.projects.flatMap((project) => {
      const role = getDemoProjectRole(persona, project.id)
      return role
        ? [
            {
              ...decorateProject(project, persona.userId),
              current_user_role: role,
            },
          ]
        : []
    })
    const integrationId = url.searchParams.get('integration_id')
    if (integrationId) {
      const repositoryIds = new Set(
        (demoState.repositories[integrationId] ?? []).map(
          (repository) => repository.id,
        ),
      )
      projects = projects.filter(
        (project) =>
          !!project.repository_id && repositoryIds.has(project.repository_id),
      )
    }
    const search = url.searchParams.get('search')?.trim().toLowerCase()
    if (search) {
      projects = projects.filter(
        (project) =>
          project.name.toLowerCase().includes(search) ||
          project.description?.toLowerCase().includes(search),
      )
    }

    const sort = url.searchParams.get('sort')
    const direction = url.searchParams.get('direction') === 'asc' ? 1 : -1
    projects.sort((left, right) => {
      if (sort === 'activity') {
        return (
          Number(right.is_favorite) - Number(left.is_favorite) ||
          (right.last_build_at ?? right.created_at) -
            (left.last_build_at ?? left.created_at) ||
          left.id.localeCompare(right.id)
        )
      }
      const leftValue =
        sort === 'name'
          ? left.name.toLowerCase()
          : sort === 'updated_at'
            ? left.updated_at
            : left.created_at
      const rightValue =
        sort === 'name'
          ? right.name.toLowerCase()
          : sort === 'updated_at'
            ? right.updated_at
            : right.created_at
      const leftString = z.string().safeParse(leftValue)
      const compared = leftString.success
        ? leftString.data.localeCompare(String(rightValue))
        : Number(leftValue) - Number(rightValue)
      return (compared || left.id.localeCompare(right.id)) * direction
    })

    const total = projects.length
    const limit = Math.min(Number(url.searchParams.get('limit')) || 50, 200)
    const offset = Number(url.searchParams.get('offset')) || 0
    return HttpResponse.json({
      projects: projects.slice(offset, offset + limit),
      total,
    })
  }),

  demoApi.getProject(async ({ params, request }) => {
    await delay(150)
    const persona = getDemoPersonaFromRequest(request)
    const role = getDemoProjectRole(persona, String(params.project_id))
    const project = demoState.projects.find((p) => p.id === params.project_id)
    if (!project || !role) {
      return HttpResponse.json(
        { error: 'Project not found', code: 'not_found' },
        { status: 404 },
      )
    }
    return HttpResponse.json({
      project: {
        ...decorateProject(project, persona.userId),
        current_user_role: role,
      },
      pipeline_count: demoState.pipelines.filter(
        (pipeline) => pipeline.project_id === project.id,
      ).length,
      build_count: demoState.builds.filter(
        (build) => build.project_id === project.id,
      ).length,
      current_user_role: role,
      setup: demoSetupProgress(project.id),
    })
  }),

  demoApi.detectProjectApp(async ({ params }) => {
    await delay(400)
    const project = demoState.projects.find((p) => p.id === params.project_id)
    if (!project) {
      return HttpResponse.json(
        { error: 'Project not found', code: 'not_found' },
        { status: 404 },
      )
    }
    return HttpResponse.json({
      reference: project.default_branch ?? 'main',
      flutter: true,
      app_name: 'coin',
      platforms: ['android', 'ios'],
      config_files: [],
    })
  }),

  demoApi.createSampleProject(async ({ request }) => {
    await delay(1200)
    const forbidden = requireDemoInstancePermission(request, 'projects:write')
    if (forbidden) return forbidden
    const persona = getDemoPersonaFromRequest(request)
    const project = {
      id: `proj-demo-sample-${crypto.randomUUID().slice(0, 8)}`,
      name: 'Sample app',
      description: 'A new Flutter app Oore created to show a first build.',
      repository_id: 'local:oore-sample',
      repository_full_name: 'oore_sample_3f9c2a1e',
      default_branch: 'main',
      settings: {},
      auto_add_pipelines: true,
      created_by: persona.userId,
      created_at: ago(0),
      updated_at: ago(0),
    }
    demoState.projects.unshift(project)
    demoState.projectRoles[project.id] = { [persona.userId]: 'maintainer' }
    return HttpResponse.json({ project }, { status: 201 })
  }),

  demoApi.listProjectMembers(async ({ params, request }) => {
    await delay(120)
    const projectId = String(params.project_id)
    const persona = getDemoPersonaFromRequest(request)
    if (!getDemoProjectRole(persona, projectId)) {
      return HttpResponse.json(
        { error: 'Project not found', code: 'not_found' },
        { status: 404 },
      )
    }
    const memberRoles = Object.entries(demoState.projectRoles[projectId] ?? {})
    return HttpResponse.json({
      members: memberRoles.map(([userId, role], index) => {
        const user = demoState.users.find(
          (candidate) => candidate.id === userId,
        )!
        return {
          id: `pm-demo-${projectId}-${index}`,
          project_id: projectId,
          user_id: userId,
          role,
          user_email: user.email,
          user_role: user.role,
          user_display_name: user.display_name,
          user_avatar_url: user.avatar_url,
          created_at: user.created_at,
          updated_at: user.updated_at,
        }
      }),
    })
  }),

  demoApi.listProjectMemberCandidates(async ({ params, request }) => {
    await delay(120)
    const projectId = String(params.project_id)
    const forbidden = requireDemoProjectPermission(
      request,
      projectId,
      'members:write',
    )
    if (forbidden) return forbidden
    return HttpResponse.json({
      candidates: demoState.users.flatMap((user) =>
        (user.role === 'developer' || user.role === 'qa_viewer') &&
        (user.status === 'active' || user.status === 'invited') &&
        !hasProjectMembership(projectId, user.id)
          ? [
              {
                id: user.id,
                email: user.email,
                display_name: user.display_name,
                role: user.role,
                status: user.status,
              },
            ]
          : [],
      ),
    })
  }),

  demoApi.addProjectMember(async ({ params, request }) => {
    await delay(200)
    const projectId = String(params.project_id)
    const forbidden = requireDemoProjectPermission(
      request,
      projectId,
      'members:write',
    )
    if (forbidden) return forbidden
    const body = projectMemberRequestSchema.parse(await request.json())
    const user = demoState.users.find(
      (candidate) => candidate.id === body.user_id,
    )
    if (!user) {
      return HttpResponse.json(
        { error: 'User not found', code: 'not_found' },
        { status: 404 },
      )
    }
    if (user.role === 'qa_viewer' && body.role !== 'viewer') {
      return HttpResponse.json(
        {
          error: 'QA users can only be project viewers.',
          code: 'invalid_input',
        },
        { status: 400 },
      )
    }
    if (user.role === 'owner' || user.role === 'admin') {
      return HttpResponse.json(
        {
          error:
            'Owner and admin users already have full access to all projects',
          code: 'invalid_user',
        },
        { status: 400 },
      )
    }
    if (demoState.projectRoles[projectId]?.[user.id]) {
      return HttpResponse.json(
        {
          error: 'User is already a member of this project',
          code: 'already_member',
        },
        { status: 409 },
      )
    }
    demoState.projectRoles[projectId] ??= {}
    demoState.projectRoles[projectId][user.id] = body.role
    return HttpResponse.json(
      {
        member: {
          id: `pm-demo-${projectId}-${user.id}`,
          project_id: projectId,
          user_id: user.id,
          role: body.role,
          user_email: user.email,
          user_role: user.role,
          user_display_name: user.display_name,
          user_avatar_url: user.avatar_url,
          created_at: ago(0),
          updated_at: ago(0),
        },
      },
      { status: 201 },
    )
  }),

  demoApi.updateProjectMember(async ({ params, request }) => {
    await delay(180)
    const projectId = String(params.project_id)
    const userId = String(params.user_id)
    const forbidden = requireDemoProjectPermission(
      request,
      projectId,
      'members:write',
    )
    if (forbidden) return forbidden
    const body = projectRoleRequestSchema.parse(await request.json())
    const user = demoState.users.find((candidate) => candidate.id === userId)
    if (!user) {
      return HttpResponse.json(
        { error: 'User not found', code: 'not_found' },
        { status: 404 },
      )
    }
    if (user.role === 'qa_viewer' && body.role !== 'viewer') {
      return HttpResponse.json(
        {
          error: 'QA users can only be project viewers.',
          code: 'invalid_input',
        },
        { status: 400 },
      )
    }
    demoState.projectRoles[projectId] ??= {}
    demoState.projectRoles[projectId][userId] = body.role
    return HttpResponse.json({
      member: {
        id: `pm-demo-${projectId}-${user.id}`,
        project_id: projectId,
        user_id: user.id,
        role: body.role,
        user_email: user.email,
        user_role: user.role,
        user_display_name: user.display_name,
        user_avatar_url: user.avatar_url,
        created_at: user.created_at,
        updated_at: ago(0),
      },
    })
  }),

  demoApi.removeProjectMember(async ({ params, request }) => {
    await delay(180)
    const projectId = String(params.project_id)
    const userId = String(params.user_id)
    const forbidden = requireDemoProjectPermission(
      request,
      projectId,
      'members:write',
    )
    if (forbidden) return forbidden
    delete demoState.projectRoles[projectId]?.[userId]
    return HttpResponse.json({ ok: true })
  }),

  demoApi.createProject(async ({ request }) => {
    await delay(300)
    const forbidden = requireDemoInstancePermission(request, 'projects:write')
    if (forbidden) return forbidden
    const persona = getDemoPersonaFromRequest(request)
    const body = createProjectRequestSchema.parse(await request.json())
    if (body.repository_id && !hasRepository(body.repository_id)) {
      return HttpResponse.json(
        { error: 'Repository not found', code: 'not_found' },
        { status: 404 },
      )
    }
    const project = {
      id: `proj-demo-new-${crypto.randomUUID().slice(0, 8)}`,
      name: body.name,
      description: body.description,
      repository_id: body.repository_id,
      default_branch: body.default_branch ?? 'main',
      settings: {},
      auto_add_pipelines: body.auto_add_pipelines ?? true,
      created_by: persona.userId,
      created_at: ago(0),
      updated_at: ago(0),
    }
    demoState.projects.unshift(project)
    demoState.projectRoles[project.id] = { [persona.userId]: 'maintainer' }
    return HttpResponse.json({ project }, { status: 201 })
  }),

  demoApi.updateProject(async ({ params, request }) => {
    await delay(200)
    const forbidden = requireDemoProjectPermission(
      request,
      String(params.project_id),
      'projects:write',
    )
    if (forbidden) return forbidden
    const body = await parseDemoJsonObject(request)
    if ('repository_id' in body) {
      const sourceForbidden = requireDemoInstancePermission(
        request,
        'projects:write',
      )
      if (sourceForbidden) return sourceForbidden
    }
    const project = demoState.projects.find((p) => p.id === params.project_id)
    if (!project) {
      return HttpResponse.json(
        { error: 'Project not found', code: 'not_found' },
        { status: 404 },
      )
    }
    const repositoryId = z.string().safeParse(body.repository_id)
    if (repositoryId.success && !hasRepository(repositoryId.data)) {
      return HttpResponse.json(
        { error: 'Repository not found', code: 'not_found' },
        { status: 404 },
      )
    }
    Object.assign(project, body, { updated_at: ago(0) })
    return HttpResponse.json({ project })
  }),

  demoApi.deleteProject(async ({ params, request }) => {
    await delay(200)
    const forbidden = requireDemoProjectPermission(
      request,
      String(params.project_id),
      'projects:delete',
    )
    if (forbidden) return forbidden
    const projectId = String(params.project_id)
    const buildIds = new Set(
      demoState.builds
        .filter((build) => build.project_id === projectId)
        .map((build) => build.id),
    )
    demoState.projects = demoState.projects.filter(
      (project) => project.id !== projectId,
    )
    demoState.pipelines = demoState.pipelines.filter(
      (pipeline) => pipeline.project_id !== projectId,
    )
    demoState.builds = demoState.builds.filter(
      (build) => build.project_id !== projectId,
    )
    for (const buildId of buildIds) {
      delete demoState.buildEvents[buildId]
      delete demoState.buildLogs[buildId]
      delete demoState.artifacts[buildId]
    }
    delete demoState.projectRoles[projectId]
    delete demoState.repositoryWorkflows[projectId]
    delete demoState.repositoryWorkflowBranches[projectId]
    return HttpResponse.json({ ok: true })
  }),
]
