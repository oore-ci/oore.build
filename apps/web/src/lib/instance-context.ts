import { redirect, type ParsedLocation } from '@tanstack/react-router'
import type { Instance } from '@/lib/types'
import type { UserRole } from '@oore/client/models'
import { loginSearchFor } from '@/lib/auth-redirect'
import { toast } from '@/lib/toast'
import { useAuthStore } from '@/stores/auth-store'
import { useInstanceStore } from '@/stores/instance-store'
import { useSetupStore } from '@/stores/setup-store'

/**
 * Read the active instance from the store. If none is active, redirect to '/'.
 * Intended for use in route `beforeLoad` guards (synchronous, no hooks).
 */
export function getActiveInstanceOrRedirect(): Instance {
  const { activeInstanceId, instances } = useInstanceStore.getState()
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- defensive: instances record may be stale
  if (activeInstanceId && instances[activeInstanceId]) {
    return instances[activeInstanceId]
  }

  throw redirect({ to: '/' })
}

/**
 * Ensure the setup store has the correct instance context loaded.
 * Call this from route `beforeLoad` so that by the time components render,
 * the setup store's sessionToken/expiresAt are hydrated from the correct
 * namespaced sessionStorage keys.
 */
export function syncSetupStoreContext(instanceId: string): void {
  const current = useSetupStore.getState().instanceId
  if (current !== instanceId) {
    useSetupStore.getState().setInstanceContext(instanceId)
  }
}

/**
 * Read the setup session token for a given instance from sessionStorage.
 * Pure synchronous read — no hooks, no store subscription.
 */
export function getSetupSessionTokenForInstance(
  instanceId: string,
): string | null {
  try {
    return sessionStorage.getItem(`oore_setup_session_${instanceId}`) ?? null
  } catch {
    return null
  }
}

/**
 * Require a setup session token for the given instance.
 * If missing, redirect to '/setup'. For use in route `beforeLoad` guards.
 */
export function requireSetupSessionOrRedirect(instanceId: string): string {
  const token = getSetupSessionTokenForInstance(instanceId)
  if (!token) {
    throw redirect({ to: '/setup' })
  }
  return token
}

/**
 * Read the auth token for a given instance from localStorage.
 * Pure synchronous read — no hooks, no store subscription.
 */
export function getAuthTokenForInstance(instanceId: string): string | null {
  try {
    return localStorage.getItem(`oore_auth_token_${instanceId}`) ?? null
  } catch {
    return null
  }
}

/**
 * Read the auth token expiry for a given instance from localStorage.
 * Returns null if missing or unparseable.
 */
function getAuthExpiresForInstance(instanceId: string): number | null {
  try {
    const val = localStorage.getItem(`oore_auth_expires_${instanceId}`)
    if (!val) return null
    const expiresAt = Number(val)
    return Number.isFinite(expiresAt) ? expiresAt : null
  } catch {
    return null
  }
}

/** Whether the instance has a stored, unexpired session token. */
export function hasValidSessionForInstance(instanceId: string): boolean {
  const expiresAt = getAuthExpiresForInstance(instanceId)
  return (
    !!getAuthTokenForInstance(instanceId) &&
    expiresAt != null &&
    expiresAt > Math.floor(Date.now() / 1000)
  )
}

/** Where a route guard sends someone, and what they were trying to open. */
interface GuardContext {
  location: Pick<ParsedLocation, 'href'>
  preload: boolean
}

/**
 * Require a non-expired auth token for the given instance.
 * If missing or expired, redirect to '/login', which returns to the requested
 * page after sign-in. For use in route `beforeLoad` guards.
 */
export function requireAuthOrRedirect(
  instanceId: string,
  { location }: Pick<GuardContext, 'location'>,
): string {
  const token = getAuthTokenForInstance(instanceId)
  if (!token || !hasValidSessionForInstance(instanceId)) {
    throw redirect({ to: '/login', search: loginSearchFor(location.href) })
  }

  // Sync the auth store context
  const current = useAuthStore.getState().instanceId
  if (current !== instanceId) {
    useAuthStore.getState().setInstanceContext(instanceId)
  }
  return token
}

/**
 * Require authentication plus one of the supplied instance roles.
 * This is a UI/direct-route guard; the API remains the authorization boundary.
 * People without access go Home and are told why.
 */
export function requireInstanceRoleOrRedirect(
  instanceId: string,
  allowedRoles: ReadonlyArray<UserRole>,
  context: GuardContext,
): string {
  const token = requireAuthOrRedirect(instanceId, context)
  const role = useAuthStore.getState().user?.role
  if (!role || !allowedRoles.includes(role)) {
    if (!context.preload) {
      const needed = allowedRoles.includes('developer') ? 'Developer' : 'Admin'
      toast.error(`You need ${needed} access to open that page.`, {
        id: 'route-access-denied',
        description:
          'Ask an owner or admin of this instance to change your role.',
      })
    }
    throw redirect({ to: '/' })
  }
  return token
}
