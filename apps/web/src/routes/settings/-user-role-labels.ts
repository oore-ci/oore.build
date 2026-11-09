interface RoleLabels {
  [role: string]: string
}

export const ROLE_LABELS: RoleLabels = {
  owner: 'Owner',
  admin: 'Admin',
  developer: 'Developer',
  qa_viewer: 'QA Viewer',
}

export const ROLE_DESCRIPTIONS: RoleLabels = {
  owner: 'Full control of this instance, including external access.',
  admin: 'Can manage users, sources, and all apps. Cannot delete the instance.',
  developer:
    'Can work on assigned apps according to their app role. Cannot create apps or manage instance settings.',
  qa_viewer:
    'Tester access to assigned app releases, install actions, and diagnostic logs.',
}
