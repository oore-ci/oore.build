import { useAuthStore } from '@/stores/auth-store'
import type { ProjectRole, UserRole } from '@oore/client/models'

type PermissionTemplate = `${string}:${string}`

/**
 * Client-side RBAC matrix mirroring crates/oored/rbac_policy.csv.
 * Used for UI gating only — the backend enforces the real policy.
 */
const ADMIN_PERMISSIONS = new Set<PermissionTemplate>([
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
])

const RBAC_MATRIX = {
  owner: ADMIN_PERMISSIONS,
  admin: ADMIN_PERMISSIONS,
  developer: new Set<PermissionTemplate>([
    'projects:read',
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
  qa_viewer: new Set<PermissionTemplate>([
    'projects:read',
    'pipelines:read',
    'builds:read',
    'artifacts:read',
    'integrations:read',
  ]),
} satisfies Record<UserRole, Set<PermissionTemplate>>

const PROJECT_RBAC_MATRIX = {
  maintainer: new Set<PermissionTemplate>([
    'builds:approve',
    'projects:write',
    'projects:delete',
    'pipelines:write',
    'pipelines:delete',
    'builds:write',
    'builds:cancel',
    'artifacts:write',
  ]),
  developer: new Set<PermissionTemplate>([
    'pipelines:write',
    'pipelines:delete',
    'builds:write',
    'builds:cancel',
    'artifacts:write',
  ]),
  viewer: new Set<PermissionTemplate>(),
} satisfies Record<ProjectRole, Set<PermissionTemplate>>

export function hasProjectPermission(
  role: ProjectRole | undefined,
  permission: PermissionTemplate,
): boolean {
  if (permission.split(':').at(1) === 'read') return role !== undefined
  return !!role && PROJECT_RBAC_MATRIX[role].has(permission)
}

export function useHasPermission(permission: PermissionTemplate) {
  const role = useAuthStore((s) => s.user?.role)

  if (!role) return false

  return RBAC_MATRIX[role].has(permission)
}

export function useHasPermissions(permissions: PermissionTemplate[]) {
  const role = useAuthStore((s) => s.user?.role)

  if (!role) return Array.from({ length: permissions.length }, () => false)

  const defaultPermissions = RBAC_MATRIX[role]

  return permissions.map((p) => defaultPermissions.has(p))
}
