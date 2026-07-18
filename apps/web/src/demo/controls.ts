import type { UserRole } from '@/lib/types'
import { queryClient } from '@/lib/query-client'
import { useAuthStore } from '@/stores/auth-store'
import { useRecentProjectsStore } from '@/stores/recent-projects-store'
import { DEMO_PERSONAS, getDemoSession } from './personas'
import type { DemoScenario } from './state'

export function activateDemoPersona(role: UserRole): boolean {
  const persona = DEMO_PERSONAS.find((candidate) => candidate.role === role)
  if (!persona) return false

  const session = getDemoSession(persona)
  queryClient.clear()
  useRecentProjectsStore.getState().clear()
  useAuthStore
    .getState()
    .setAuth(session.session_token, session.expires_at, session.user, 'local')
  return true
}

export function demoScenarioUrl(
  currentHref: string,
  scenario: DemoScenario,
): string {
  const url = new URL(currentHref)
  url.searchParams.set('demoScenario', scenario)
  return url.toString()
}
