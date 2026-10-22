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
  admin:
    'Can manage users, integrations, and all projects. Cannot delete the instance.',
  developer:
    'Can work on assigned projects according to their project role. Cannot create projects or manage instance settings.',
  qa_viewer:
    'Tester access to assigned project releases, install actions, and diagnostic logs.',
}
