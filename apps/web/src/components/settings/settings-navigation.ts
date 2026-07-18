import {
  Archive02Icon,
  Audit01Icon,
  CpuIcon,
  Delete02Icon,
  Key01Icon,
  Link04Icon,
  Notification03Icon,
  Settings01Icon,
  Sun03Icon,
  UserMultiple02Icon,
} from '@hugeicons/core-free-icons'

import type { UserRole } from '@/lib/types'

const ADMIN_ROLES: ReadonlyArray<UserRole> = ['owner', 'admin']
const OPERATOR_ROLES: ReadonlyArray<UserRole> = ['owner', 'admin', 'developer']

const SETTINGS_GROUPS = [
  {
    title: 'Personal',
    items: [
      {
        title: 'Theme',
        description: 'Color and light or dark mode.',
        to: '/settings/theme',
        icon: Sun03Icon,
        roles: OPERATOR_ROLES,
      },
    ],
  },
  {
    title: 'Instance',
    items: [
      {
        title: 'General',
        description: 'Runtime, External Access, and service updates.',
        to: '/settings/preferences',
        icon: Settings01Icon,
        roles: ADMIN_ROLES,
      },
      {
        title: 'Runners',
        description: 'Runner health, metadata, and Direct runner policy.',
        to: '/settings/runners',
        icon: CpuIcon,
        roles: OPERATOR_ROLES,
      },
      {
        title: 'Sources',
        description: 'Connected repositories and provider credentials.',
        to: '/settings/integrations',
        icon: Link04Icon,
        roles: OPERATOR_ROLES,
      },
      {
        title: 'Artifact storage',
        description: 'Local or S3-compatible artifact persistence.',
        to: '/settings/artifacts',
        icon: Archive02Icon,
        roles: ADMIN_ROLES,
      },
      {
        title: 'Retention',
        description: 'Cleanup policy for builds, logs, and artifacts.',
        to: '/settings/retention',
        icon: Delete02Icon,
        roles: ADMIN_ROLES,
      },
    ],
  },
  {
    title: 'Access',
    items: [
      {
        title: 'Users',
        description: 'Instance roles and project access.',
        to: '/settings/users',
        icon: UserMultiple02Icon,
        roles: ADMIN_ROLES,
      },
      {
        title: 'API tokens',
        description: 'Personal credentials for automation and tools.',
        to: '/settings/api-tokens',
        icon: Key01Icon,
        roles: OPERATOR_ROLES,
      },
    ],
  },
  {
    title: 'Delivery',
    items: [
      {
        title: 'Notifications',
        description: 'Build and system notification channels.',
        to: '/settings/notifications',
        icon: Notification03Icon,
        roles: ADMIN_ROLES,
      },
    ],
  },
  {
    title: 'History',
    items: [
      {
        title: 'Audit log',
        description: 'Security and administrative activity.',
        to: '/settings/audit-log',
        icon: Audit01Icon,
        roles: ADMIN_ROLES,
      },
    ],
  },
] as const

export function settingsGroupsForRole(role: UserRole | undefined) {
  if (!role) return []

  return SETTINGS_GROUPS.map((group) => ({
    ...group,
    items: group.items.filter((item) => item.roles.includes(role)),
  })).filter((group) => group.items.length > 0)
}

export function canAccessSettings(role: UserRole | undefined): boolean {
  return settingsGroupsForRole(role).length > 0
}
