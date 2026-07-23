import { HttpResponse } from 'msw'

import type { DemoPersona } from './personas'
import { getDemoPersonaFromRequest, getDemoProjectRole } from './personas'
import { demoState } from './state'

const INSTANCE_PERMISSIONS: Record<string, ReadonlySet<string>> = {
  owner: new Set(['*']),
  admin: new Set(['*']),
  developer: new Set([
    'pipelines:write',
    'pipelines:delete',
    'builds:write',
    'builds:cancel',
    'artifacts:write',
    'api_tokens:write',
    'api_tokens:delete',
  ]),
  qa_viewer: new Set(),
}

const PROJECT_PERMISSIONS: Record<string, ReadonlySet<string>> = {
  maintainer: new Set([
    'projects:write',
    'projects:delete',
    'members:write',
    'pipelines:write',
    'pipelines:delete',
    'builds:write',
    'builds:cancel',
    'artifacts:write',
  ]),
  developer: new Set([
    'pipelines:write',
    'pipelines:delete',
    'builds:write',
    'builds:cancel',
    'artifacts:write',
  ]),
  viewer: new Set(),
}

function canUseInstancePermission(
  persona: DemoPersona,
  permission: string,
): boolean {
  const permissions = INSTANCE_PERMISSIONS[persona.role]
  return permissions.has('*') || permissions.has(permission)
}

function forbidden(): Response {
  return HttpResponse.json(
    {
      error: 'You do not have permission to access this resource.',
      code: 'forbidden',
    },
    { status: 403 },
  )
}

export function requireDemoInstancePermission(
  request: Request,
  permission: string,
): Response | null {
  return canUseInstancePermission(
    getDemoPersonaFromRequest(request),
    permission,
  )
    ? null
    : forbidden()
}

export function requireDemoProjectPermission(
  request: Request,
  projectId: string,
  permission: string,
): Response | null {
  const persona = getDemoPersonaFromRequest(request)
  if (!demoState.projects.some((project) => project.id === projectId)) {
    return HttpResponse.json(
      { error: 'Project not found', code: 'not_found' },
      { status: 404 },
    )
  }
  if (persona.role === 'owner' || persona.role === 'admin') return null
  const assignedRole = getDemoProjectRole(persona, projectId)
  const projectRole =
    persona.role === 'qa_viewer' && assignedRole ? 'viewer' : assignedRole
  return projectRole && PROJECT_PERMISSIONS[projectRole].has(permission)
    ? null
    : forbidden()
}
