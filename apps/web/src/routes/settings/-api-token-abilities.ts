import type { TokenAbility } from '@oore/client/models'

export const TOKEN_ABILITIES: ReadonlyArray<{
  value: TokenAbility
  label: string
  description: string
}> = [
  {
    value: 'read',
    label: 'Read',
    description: 'Apps, pipelines, builds, logs, artifacts and releases.',
  },
  {
    value: 'build',
    label: 'Build',
    description:
      'Start, cancel, re-run and retry builds, set release notes, send to testers.',
  },
  {
    value: 'release_operate',
    label: 'Pause and halt releases',
    description: 'Stop a store rollout that is going wrong.',
  },
  {
    value: 'release_manage',
    label: 'Manage releases',
    description:
      'Raise, resume, complete, approve and promote releases, and approve gated builds.',
  },
  {
    value: 'manage',
    label: 'Manage settings',
    description: 'Pipelines, secrets, signing, members, settings and tokens.',
  },
]

export const DEFAULT_TOKEN_ABILITIES: Array<TokenAbility> = ['read', 'build']

export const TOKEN_ABILITY_VALUES = [
  'read',
  'build',
  'release_operate',
  'release_manage',
  'manage',
] as const satisfies ReadonlyArray<TokenAbility>

export function tokenAbilityLabel(ability: TokenAbility): string {
  return (
    TOKEN_ABILITIES.find((candidate) => candidate.value === ability)?.label ??
    ability
  )
}
