import type { ProjectRole } from '@/lib/types'
import type { DemoPersona } from './state'
import { DEMO_PASSWORD } from './seed'
import { DEMO_PERSONAS, demoSessionExpiresAt, demoState } from './state'

export { DEMO_PERSONAS, type DemoPersona }

export function authenticateDemoUser(
  email: string,
  password: string,
): DemoPersona | null {
  if (password !== DEMO_PASSWORD) return null
  const normalizedEmail = email.trim().toLowerCase()
  return (
    DEMO_PERSONAS.find((persona) => persona.email === normalizedEmail) ?? null
  )
}

export function getDemoPersonaByToken(
  token?: string | null,
): DemoPersona | null {
  if (!token) return null
  return demoState.personas.find((persona) => persona.token === token) ?? null
}

export function getDemoPersonaFromRequest(request: Request): DemoPersona {
  const authorization = request.headers.get('Authorization')
  const persona = getDemoPersonaByToken(
    authorization?.replace(/^Bearer\s+/i, ''),
  )
  if (!persona) throw new Error('Demo request reached a handler without auth')
  return persona
}

export function getDemoProjectRole(
  persona: DemoPersona,
  projectId: string,
): ProjectRole | null {
  if (!demoState.projects.some((project) => project.id === projectId)) {
    return null
  }
  if (persona.role === 'owner' || persona.role === 'admin') {
    return demoState.projectRoles[projectId]?.[persona.userId] ?? 'maintainer'
  }
  const role = demoState.projectRoles[projectId]?.[persona.userId] ?? null
  if (persona.role === 'qa_viewer' && role !== null) return 'viewer'
  return role
}

export function getDemoSession(persona: DemoPersona) {
  return {
    session_token: persona.token,
    expires_at: demoSessionExpiresAt(),
    user: {
      email: persona.email,
      oidc_subject: `demo::${persona.role}`,
      user_id: persona.userId,
      role: persona.role,
    },
  }
}
