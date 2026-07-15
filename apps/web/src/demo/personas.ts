import type { ProjectRole, UserRole } from '@/lib/types'
import {
  DEMO_AUTH_EXPIRES_AT,
  DEMO_PASSWORD,
  PROJECT_IDS,
  USER_IDS,
} from './seed'

export interface DemoPersona {
  userId: string
  email: string
  displayName: string
  role: UserRole
  token: string
  projectRoles: Partial<Record<string, ProjectRole>>
}

export const DEMO_PERSONAS: Array<DemoPersona> = [
  {
    userId: USER_IDS.owner,
    email: 'demo+owner@oore.build',
    displayName: 'Alex Chen',
    role: 'owner',
    token: 'demo-session-token-owner',
    projectRoles: {
      [PROJECT_IDS.flutterShop]: 'maintainer',
      [PROJECT_IDS.internalAdmin]: 'maintainer',
      [PROJECT_IDS.nativePayments]: 'maintainer',
    },
  },
  {
    userId: USER_IDS.admin,
    email: 'demo+admin@oore.build',
    displayName: 'Jamie Park',
    role: 'admin',
    token: 'demo-session-token-admin',
    projectRoles: {
      [PROJECT_IDS.flutterShop]: 'maintainer',
      [PROJECT_IDS.internalAdmin]: 'maintainer',
      [PROJECT_IDS.nativePayments]: 'maintainer',
    },
  },
  {
    userId: USER_IDS.developer,
    email: 'demo+developer@oore.build',
    displayName: 'Morgan Lee',
    role: 'developer',
    token: 'demo-session-token-developer',
    projectRoles: {
      [PROJECT_IDS.flutterShop]: 'developer',
      [PROJECT_IDS.nativePayments]: 'viewer',
    },
  },
  {
    userId: USER_IDS.qaViewer,
    email: 'demo+qa@oore.build',
    displayName: 'Taylor Ruiz',
    role: 'qa_viewer',
    token: 'demo-session-token-qa',
    projectRoles: {
      [PROJECT_IDS.flutterShop]: 'viewer',
      [PROJECT_IDS.nativePayments]: 'viewer',
    },
  },
]

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

export function getDemoPersonaByToken(token?: string | null): DemoPersona {
  return (
    DEMO_PERSONAS.find((persona) => persona.token === token) ?? DEMO_PERSONAS[0]
  )
}

export function getDemoPersonaFromRequest(request: Request): DemoPersona {
  const authorization = request.headers.get('Authorization')
  return getDemoPersonaByToken(authorization?.replace(/^Bearer\s+/i, ''))
}

export function getDemoProjectRole(
  persona: DemoPersona,
  projectId: string,
): ProjectRole | null {
  return persona.projectRoles[projectId] ?? null
}

export function getDemoSession(persona: DemoPersona) {
  return {
    session_token: persona.token,
    expires_at: DEMO_AUTH_EXPIRES_AT,
    user: {
      email: persona.email,
      oidc_subject: `demo::${persona.role}`,
      user_id: persona.userId,
      role: persona.role,
    },
  }
}
