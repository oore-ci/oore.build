import { useAuthStore } from '@/stores/auth-store'
import type { ProjectRole, UserRole } from '@/lib/types'

/**
 * Client-side RBAC matrix mirroring crates/oored/rbac_policy.csv.
 * Used for UI gating only — the backend enforces the real policy.
 */
const RBAC_MATRIX: Record<string, Set<string>> = {
  owner: new Set([
    'instance_settings:read',
    'instance_settings:write',
    'users:read',
    'users:write',
    'users:invite',
    'users:delete',
    'users:enable',
    'projects:read',
    'projects:write',
    'projects:delete',
    'pipelines:read',
    'pipelines:write',
    'pipelines:delete',
    'builds:read',
    'builds:write',
    'builds:cancel',
    'artifacts:read',
    'artifacts:write',
    'artifacts:delete',
    'runners:read',
    'runners:write',
    'runners:delete',
    'integrations:read',
    'integrations:write',
    'integrations:delete',
    'api_tokens:read',
    'api_tokens:write',
    'api_tokens:delete',
    'audit_logs:read',
  ]),
  admin: new Set([
    'instance_settings:read',
    'instance_settings:write',
    'users:read',
    'users:write',
    'users:invite',
    'users:delete',
    'users:enable',
    'projects:read',
    'projects:write',
    'projects:delete',
    'pipelines:read',
    'pipelines:write',
    'pipelines:delete',
    'builds:read',
    'builds:write',
    'builds:cancel',
    'artifacts:read',
    'artifacts:write',
    'artifacts:delete',
    'runners:read',
    'runners:write',
    'runners:delete',
    'integrations:read',
    'integrations:write',
    'integrations:delete',
    'api_tokens:read',
    'api_tokens:write',
    'api_tokens:delete',
    'audit_logs:read',
  ]),
  developer: new Set([
    'projects:read',
    'projects:write',
    'pipelines:read',
    'pipelines:write',
    'builds:read',
    'builds:write',
    'builds:cancel',
    'artifacts:read',
    'artifacts:write',
    'runners:read',
    'integrations:read',
    'api_tokens:read',
    'api_tokens:write',
    'api_tokens:delete',
  ]),
  qa_viewer: new Set([
    'projects:read',
    'pipelines:read',
    'builds:read',
    'artifacts:read',
    'integrations:read',
  ]),
}

const PROJECT_RBAC_MATRIX: Record<ProjectRole, Set<string>> = {
  maintainer: new Set([
    'projects:write',
    'projects:delete',
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

export function hasProjectPermission(
  role: ProjectRole | undefined,
  resource: string,
  action: string,
): boolean {
  if (action === 'read') return role !== undefined
  return !!role && PROJECT_RBAC_MATRIX[role].has(`${resource}:${action}`)
}

export function hasInstancePermission(
  role: UserRole | undefined,
  resource: string,
  action: string,
): boolean {
  return !!role && RBAC_MATRIX[role].has(`${resource}:${action}`)
}

export function useHasPermission(resource: string, action: string): boolean {
  const role = useAuthStore((s) => s.user?.role)
  return hasInstancePermission(role, resource, action)
}
