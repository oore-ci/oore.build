import {
  Archive02Icon,
  Audit01Icon,
  CpuIcon,
  Key01Icon,
  Link04Icon,
  LockPasswordIcon,
  Notification03Icon,
  Settings01Icon,
  UserGroupIcon,
  UserMultiple02Icon,
} from '@hugeicons/core-free-icons'

import type { UserRole } from '@oore/client/models'

const ADMIN_ROLES: ReadonlyArray<UserRole> = ['owner', 'admin']
const OPERATOR_ROLES: ReadonlyArray<UserRole> = ['owner', 'admin', 'developer']

/**
 * Settings pages in four small groups. The first has no label: General sits
 * on top on its own. Every page stays one click away; there is no hub.
 */
const SETTINGS_GROUPS = [
  {
    title: null,
    items: [
      {
        title: 'General',
        description: 'Runtime, External Access, and service updates.',
        to: '/settings/general',
        icon: Settings01Icon,
        roles: ADMIN_ROLES,
      },
    ],
  },
  {
    title: 'Build',
    items: [
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
        to: '/settings/sources',
        icon: Link04Icon,
        roles: OPERATOR_ROLES,
      },
      {
        title: 'Storage',
        description:
          'Artifact storage and retention: where builds are kept and when they are cleaned up.',
        to: '/settings/artifacts',
        icon: Archive02Icon,
        roles: ADMIN_ROLES,
      },
    ],
  },
  {
    title: 'Release',
    items: [
      {
        title: 'Secrets and credentials',
        description:
          'Secrets and store credentials (App Store Connect, Google Play keys) every build can use.',
        to: '/settings/secrets',
        icon: LockPasswordIcon,
        roles: ADMIN_ROLES,
      },
      {
        title: 'Notifications',
        description: 'Build and system notification channels.',
        to: '/settings/notifications',
        icon: Notification03Icon,
        roles: ADMIN_ROLES,
      },
      {
        title: 'Tester groups',
        description: 'People a build can be sent to in one step.',
        to: '/settings/testers',
        icon: UserGroupIcon,
        roles: ADMIN_ROLES,
      },
    ],
  },
  {
    title: 'Access',
    items: [
      {
        title: 'Users',
        description: 'Instance roles and app access.',
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
