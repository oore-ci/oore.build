import { useState } from 'react'

import type { SigningIdentity, SigningIdentityKind } from '@oore/client/models'
import { useSigningIdentities } from '@/hooks/use-signing-identities'
import { signingIdentitySummary } from '@/lib/signing-identities'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { SigningIdentityDialog } from './signing-identity-dialog'

/** Select value that opens the add dialog instead of picking. */
const NEW_IDENTITY = '__new__'
/** Select value for the empty pick, which the form keeps as `''`. */
const NONE = '__none__'

const NEW_LABELS = {
  android_keystore: 'New keystore…',
  ios_distribution: 'New certificate…',
} satisfies Record<SigningIdentityKind, string>

/** `coin-upload · coin-upload.jks · alias upload` */
export function signingIdentityOptionLabel(identity: SigningIdentity) {
  const summary = signingIdentitySummary(identity)
  return summary ? `${identity.name} · ${summary}` : identity.name
}

/**
 * Picks an identity from the app's signing library, or adds one in place:
 * the last option opens the add dialog and picks what it saves.
 */
export function SigningIdentityPicker({
  canAdd = true,
  emptyLabel,
  extra = [],
  id,
  invalid,
  kind,
  onChange,
  projectId,
  value,
}: {
  canAdd?: boolean
  /** The label of the empty value, such as `Not signed`. */
  emptyLabel: string
  /** More choices before the library's, such as files a pipeline holds. */
  extra?: ReadonlyArray<readonly [string, string]>
  id?: string
  invalid?: boolean
  kind: SigningIdentityKind
  onChange: (value: string) => void
  projectId: string
  value: string
}) {
  const [adding, setAdding] = useState(false)
  const identitiesQuery = useSigningIdentities(projectId)
  const choices = (identitiesQuery.data ?? []).filter(
    (identity) => identity.kind === kind,
  )
  const entries: Array<[string, string]> = [
    [NONE, emptyLabel],
    ...extra.map(([key, label]): [string, string] => [key, label]),
    ...choices.map((identity): [string, string] => [
      identity.id,
      signingIdentityOptionLabel(identity),
    ]),
  ]
  // Keep a pick visible while the library loads.
  if (value && !entries.some(([key]) => key === value)) {
    entries.push([value, identitiesQuery.isLoading ? 'Loading…' : value])
  }
  const items = Object.fromEntries([
    ...entries,
    [NEW_IDENTITY, NEW_LABELS[kind]],
  ])

  return (
    <>
      <Select
        value={value || NONE}
        onValueChange={(next) => {
          if (next === NEW_IDENTITY) {
            setAdding(true)
            return
          }
          onChange(!next || next === NONE ? '' : next)
        }}
        items={items}
      >
        <SelectTrigger id={id} className="w-full" aria-invalid={invalid}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            {entries.map(([key, label]) => (
              <SelectItem key={key} value={key}>
                {label}
              </SelectItem>
            ))}
          </SelectGroup>
          {canAdd ? (
            <>
              <SelectSeparator />
              <SelectGroup>
                <SelectItem value={NEW_IDENTITY}>{NEW_LABELS[kind]}</SelectItem>
              </SelectGroup>
            </>
          ) : null}
        </SelectContent>
      </Select>
      <SigningIdentityDialog
        open={adding}
        onOpenChange={setAdding}
        projectId={projectId}
        kind={kind}
        onCreated={(identity) => onChange(identity.id)}
      />
    </>
  )
}
