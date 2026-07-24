import * as React from 'react'
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
} from '@/components/ui/combobox'

type IssuerPresetKind = 'quick' | 'template'

type IssuerPreset = {
  label: string
  value: string
  kind: IssuerPresetKind
  hint?: string
}

const ISSUER_PRESETS: ReadonlyArray<IssuerPreset> = [
  {
    label: 'Google',
    value: 'https://accounts.google.com',
    kind: 'quick',
  },
  {
    label: 'Microsoft Entra ID (common)',
    value: 'https://login.microsoftonline.com/common/v2.0',
    kind: 'quick',
    hint: 'Use a tenant-specific URL for strict single-tenant setups.',
  },
  {
    label: 'Apple',
    value: 'https://appleid.apple.com',
    kind: 'quick',
  },
  {
    label: 'GitLab.com',
    value: 'https://gitlab.com',
    kind: 'quick',
  },
  {
    label: 'Okta (template)',
    value: 'https://your-domain.okta.com/oauth2/default',
    kind: 'template',
    hint: 'Replace your-domain and (if needed) the authorization server.',
  },
  {
    label: 'Auth0 (template)',
    value: 'https://your-tenant.us.auth0.com',
    kind: 'template',
    hint: 'Replace your tenant and region.',
  },
  {
    label: 'Keycloak (template)',
    value: 'https://your-keycloak.example.com/realms/your-realm',
    kind: 'template',
    hint: 'Replace host and realm.',
  },
] as const

const ISSUER_PRESETS_BY_VALUE = new Map(
  ISSUER_PRESETS.map((preset) => [preset.value, preset]),
)

export type OidcIssuerUrlAutocompleteProps = Omit<
  React.ComponentProps<'input'>,
  'value' | 'defaultValue'
> & {
  value: string
  onValueChange: (next: string) => void
}

export const OidcIssuerUrlAutocomplete = React.forwardRef<
  HTMLInputElement,
  OidcIssuerUrlAutocompleteProps
>(function OidcIssuerUrlAutocomplete(
  { value, onValueChange, className, disabled, ...inputProps },
  forwardedRef,
) {
  return (
    <Combobox
      items={ISSUER_PRESETS.map((preset) => preset.value)}
      value={value}
      inputValue={value}
      onInputValueChange={onValueChange}
      onValueChange={(next) => {
        if (typeof next === 'string') onValueChange(next)
      }}
      openOnInputClick
      itemToStringValue={(item) => item}
      filter={(item, query) => {
        const preset = ISSUER_PRESETS_BY_VALUE.get(item)
        const q = query.trim().toLowerCase()
        if (!q) return true
        return (
          preset?.label.toLowerCase().includes(q) ||
          item.toLowerCase().includes(q)
        )
      }}
    >
      <ComboboxInput
        {...inputProps}
        ref={forwardedRef}
        type="url"
        inputMode="url"
        autoCapitalize="none"
        autoCorrect="off"
        spellCheck={false}
        disabled={disabled}
        showTrigger={false}
        placeholder="https://accounts.google.com"
        className={className}
      />

      <ComboboxContent sideOffset={4}>
        <ComboboxEmpty>No matches. Enter a custom issuer URL.</ComboboxEmpty>
        <ComboboxList>
          {(item: string) => {
            const preset = ISSUER_PRESETS_BY_VALUE.get(item)
            if (!preset) return null
            return (
              <ComboboxItem
                key={`${preset.kind}:${preset.label}`}
                value={item}
                className="items-start pr-2"
              >
                <span className="flex min-w-0 flex-col gap-0.5">
                  <span>{preset.label}</span>
                  <span className="font-mono text-xs break-all text-muted-foreground">
                    {preset.value}
                  </span>
                  {preset.hint ? (
                    <span className="text-[11px] text-muted-foreground">
                      {preset.hint}
                    </span>
                  ) : null}
                </span>
              </ComboboxItem>
            )
          }}
        </ComboboxList>
      </ComboboxContent>
    </Combobox>
  )
})
