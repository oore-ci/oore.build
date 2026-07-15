import { HttpResponse, delay, http } from 'msw'
import { demoProjects } from '../data/projects'
import { demoPipelines } from '../data/pipelines'
import { demoBuilds } from '../data/builds'
import { demoUsers } from '../data/users'
import { USER_IDS, ago } from '../seed'
import {
  DEMO_PERSONAS,
  getDemoPersonaFromRequest,
  getDemoProjectRole,
} from '../personas'

export const projectHandlers = [
  http.get('/v1/projects', async ({ request }) => {
    await delay(150)
    const persona = getDemoPersonaFromRequest(request)
    const projects = demoProjects.flatMap((project) => {
      const role = getDemoProjectRole(persona, project.id)
      return role ? [{ ...project, current_user_role: role }] : []
    })
    return HttpResponse.json({
      projects,
      total: projects.length,
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
      return role ? [[candidate.userId, role] as const] : []
    })
    return HttpResponse.json({
      members: memberRoles.map(([userId, role], index) => {
        const user = demoUsers.find((candidate) => candidate.id === userId)!
        return {
          id: `pm-demo-${projectId}-${index}`,
          project_id: projectId,
          user_id: userId,
          role,
          user_email: user.email,
          user_display_name: user.display_name,
          user_avatar_url: user.avatar_url,
          created_at: user.created_at,
          updated_at: user.updated_at,
        }
      }),
    })
  }),

  http.post('/v1/projects', async ({ request }) => {
    await delay(300)
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
        created_by: USER_IDS.owner,
        created_at: ago(0),
        updated_at: ago(0),
      },
    })
  }),

  http.patch('/v1/projects/:projectId', async ({ params, request }) => {
    await delay(200)
    const body = (await request.json()) as Record<string, unknown>
    const project = demoProjects.find((p) => p.id === params.projectId)
    return HttpResponse.json({
      project: project
        ? { ...project, ...body, updated_at: ago(0) }
        : { id: params.projectId, ...body, updated_at: ago(0) },
    })
  }),

  http.delete('/v1/projects/:projectId', async () => {
    await delay(200)
    return new HttpResponse(null, { status: 204 })
  }),
]
