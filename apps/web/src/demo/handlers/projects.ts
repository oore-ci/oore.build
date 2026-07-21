import { HttpResponse, delay, http } from 'msw'
import { demoProjects } from '../data/projects'
import { demoPipelines } from '../data/pipelines'
import { demoBuilds } from '../data/builds'
import { demoUsers } from '../data/users'
import { ago } from '../seed'
import {
  DEMO_PERSONAS,
  getDemoPersonaFromRequest,
  getDemoProjectRole,
} from '../personas'
import {
  requireDemoInstancePermission,
  requireDemoProjectPermission,
} from '../authorization'

const addedMembers = new Map<string, Map<string, string>>()
const memberRoleOverrides = new Map<string, string>()
const removedMembers = new Set<string>()

function memberKey(projectId: string, userId: string): string {
  return `${projectId}:${userId}`
}

function hasProjectMembership(projectId: string, userId: string): boolean {
  const key = memberKey(projectId, userId)
  const persona = DEMO_PERSONAS.find((candidate) => candidate.userId === userId)
  return (
    addedMembers.get(projectId)?.has(userId) === true ||
    (!!persona &&
      !!getDemoProjectRole(persona, projectId) &&
      !removedMembers.has(key))
  )
}

export const projectHandlers = [
  http.get('/v1/projects', async ({ request }) => {
    await delay(150)
    const url = new URL(request.url)
    const persona = getDemoPersonaFromRequest(request)
    let projects = demoProjects.flatMap((project) => {
      const role = getDemoProjectRole(persona, project.id)
      return role ? [{ ...project, current_user_role: role }] : []
    })
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
      const compared =
        typeof leftValue === 'string'
          ? leftValue.localeCompare(String(rightValue))
          : leftValue - Number(rightValue)
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

  http.get('/v1/projects/:projectId', async ({ params, request }) => {
    await delay(150)
    const persona = getDemoPersonaFromRequest(request)
    const role = getDemoProjectRole(persona, String(params.projectId))
    const project = demoProjects.find((p) => p.id === params.projectId)
    if (!project || !role) {
      return HttpResponse.json(
        { error: 'Project not found', code: 'not_found' },
        { status: 404 },
      )
    }
    return HttpResponse.json({
      project: { ...project, current_user_role: role },
      pipeline_count: demoPipelines.filter((p) => p.project_id === project.id)
        .length,
      build_count: demoBuilds.filter((b) => b.project_id === project.id).length,
      current_user_role: role,
    })
  }),

  http.get('/v1/projects/:projectId/members', async ({ params, request }) => {
    await delay(120)
    const projectId = String(params.projectId)
    const persona = getDemoPersonaFromRequest(request)
    if (!getDemoProjectRole(persona, projectId)) {
      return HttpResponse.json(
        { error: 'Project not found', code: 'not_found' },
        { status: 404 },
      )
    }
    const memberRoles = DEMO_PERSONAS.flatMap((candidate) => {
      const role = getDemoProjectRole(candidate, projectId)
      const key = memberKey(projectId, candidate.userId)
      return role && !removedMembers.has(key)
        ? [[candidate.userId, memberRoleOverrides.get(key) ?? role] as const]
        : []
    })
    for (const [userId, role] of addedMembers.get(projectId) ?? []) {
      if (!memberRoles.some(([candidateId]) => candidateId === userId)) {
        memberRoles.push([userId, role])
      }
    }
    return HttpResponse.json({
      members: memberRoles.map(([userId, role], index) => {
        const user = demoUsers.find((candidate) => candidate.id === userId)!
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

  http.get(
    '/v1/projects/:projectId/members/candidates',
    async ({ params, request }) => {
      await delay(120)
      const projectId = String(params.projectId)
      const forbidden = requireDemoProjectPermission(
        request,
        projectId,
        'members:write',
      )
      if (forbidden) return forbidden
      return HttpResponse.json({
        candidates: demoUsers.flatMap((user) =>
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
    },
  ),

  http.post('/v1/projects/:projectId/members', async ({ params, request }) => {
    await delay(200)
    const projectId = String(params.projectId)
    const forbidden = requireDemoProjectPermission(
      request,
      projectId,
      'members:write',
    )
    if (forbidden) return forbidden
    const body = (await request.json()) as { user_id: string; role: string }
    const user = demoUsers.find((candidate) => candidate.id === body.user_id)
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
    const projectMembers = addedMembers.get(projectId) ?? new Map()
    projectMembers.set(user.id, body.role)
    addedMembers.set(projectId, projectMembers)
    removedMembers.delete(memberKey(projectId, user.id))
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
        created_at: ago(0),
        updated_at: ago(0),
      },
    })
  }),

  http.patch(
    '/v1/projects/:projectId/members/:userId',
    async ({ params, request }) => {
      await delay(180)
      const projectId = String(params.projectId)
      const userId = String(params.userId)
      const forbidden = requireDemoProjectPermission(
        request,
        projectId,
        'members:write',
      )
      if (forbidden) return forbidden
      const body = (await request.json()) as { role: string }
      const user = demoUsers.find((candidate) => candidate.id === userId)
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
      memberRoleOverrides.set(memberKey(projectId, userId), body.role)
      addedMembers.get(projectId)?.set(userId, body.role)
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
    },
  ),

  http.delete(
    '/v1/projects/:projectId/members/:userId',
    async ({ params, request }) => {
      await delay(180)
      const projectId = String(params.projectId)
      const userId = String(params.userId)
      const forbidden = requireDemoProjectPermission(
        request,
        projectId,
        'members:write',
      )
      if (forbidden) return forbidden
      addedMembers.get(projectId)?.delete(userId)
      memberRoleOverrides.delete(memberKey(projectId, userId))
      removedMembers.add(memberKey(projectId, userId))
      return HttpResponse.json({ ok: true })
    },
  ),

  http.post('/v1/projects', async ({ request }) => {
    await delay(300)
    const forbidden = requireDemoInstancePermission(request, 'projects:write')
    if (forbidden) return forbidden
    const persona = getDemoPersonaFromRequest(request)
    const body = (await request.json()) as {
      name: string
      description?: string
      repository_id?: string
      default_branch?: string
    }
    return HttpResponse.json({
      project: {
        id: `proj-demo-new-${Date.now()}`,
        name: body.name,
        description: body.description,
        repository_id: body.repository_id,
        default_branch: body.default_branch ?? 'main',
        settings: {},
        created_by: persona.userId,
        created_at: ago(0),
        updated_at: ago(0),
      },
    })
  }),

  http.patch('/v1/projects/:projectId', async ({ params, request }) => {
    await delay(200)
    const forbidden = requireDemoProjectPermission(
      request,
      String(params.projectId),
      'projects:write',
    )
    if (forbidden) return forbidden
    const body = (await request.json()) as Record<string, unknown>
    if ('repository_id' in body) {
      const sourceForbidden = requireDemoInstancePermission(
        request,
        'projects:write',
      )
      if (sourceForbidden) return sourceForbidden
    }
    const project = demoProjects.find((p) => p.id === params.projectId)
    return HttpResponse.json({
      project: project
        ? { ...project, ...body, updated_at: ago(0) }
        : { id: params.projectId, ...body, updated_at: ago(0) },
    })
  }),

  http.delete('/v1/projects/:projectId', async ({ params, request }) => {
    await delay(200)
    const forbidden = requireDemoProjectPermission(
      request,
      String(params.projectId),
      'projects:delete',
    )
    if (forbidden) return forbidden
    return new HttpResponse(null, { status: 204 })
  }),
]
