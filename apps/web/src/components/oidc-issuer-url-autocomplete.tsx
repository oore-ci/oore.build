import * as React from 'react'
import { Autocomplete as AutocompletePrimitive } from '@base-ui/react/autocomplete'

import { cn } from '@/lib/utils'

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
    <AutocompletePrimitive.Root
      items={ISSUER_PRESETS}
      value={value}
      onValueChange={(next) => onValueChange(next)}
      openOnInputClick
      itemToStringValue={(item) => item.value}
      filter={(item, query) => {
        const q = query.trim().toLowerCase()
        if (!q) return true
        return (
          item.label.toLowerCase().includes(q) ||
          item.value.toLowerCase().includes(q)
        )
      }}
    >
      <AutocompletePrimitive.Input
        {...inputProps}
        ref={forwardedRef}
        type="url"
        inputMode="url"
        autoCapitalize="none"
        autoCorrect="off"
        spellCheck={false}
        disabled={disabled}
        placeholder="https://accounts.google.com"
        className={cn(
          'border-input focus-visible:border-ring focus-visible:ring-ring aria-invalid:ring-destructive/30 aria-invalid:border-destructive h-9 rounded-md border bg-background px-2.5 py-1 text-base transition-[border-color,box-shadow] focus-visible:ring-3 aria-invalid:ring-3 md:text-sm placeholder:text-muted-foreground w-full min-w-0 outline-none disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50',
          className,
        )}
      />

      <AutocompletePrimitive.Portal>
        <AutocompletePrimitive.Positioner
          side="bottom"
          sideOffset={4}
          align="start"
          className="isolate z-50"
        >
          <AutocompletePrimitive.Popup
            className={cn(
              'bg-popover text-popover-foreground ring-foreground/10 w-(--anchor-width) origin-(--transform-origin) rounded-md shadow-md ring-1',
              'data-open:animate-in data-closed:animate-out data-closed:fade-out-0 data-open:fade-in-0',
              'data-closed:zoom-out-95 data-open:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=top]:slide-in-from-bottom-2',
              'max-h-(--available-height) overflow-x-hidden overflow-y-auto p-1 duration-100',
            )}
          >
            <AutocompletePrimitive.Empty className="text-muted-foreground px-2 py-2 text-xs">
              No matches. Enter a custom issuer URL.
            </AutocompletePrimitive.Empty>

            <AutocompletePrimitive.List className="space-y-0.5">
              {(item: IssuerPreset) => (
                <AutocompletePrimitive.Item
                  key={`${item.kind}:${item.label}`}
                  value={item}
                  className={cn(
                    'relative flex w-full cursor-default select-none flex-col gap-0.5 rounded-sm px-2 py-1.5 outline-none',
                    'data-disabled:pointer-events-none data-disabled:opacity-50',
                    'data-highlighted:bg-accent data-highlighted:text-accent-foreground',
                  )}
                >
                  <span className="text-sm">{item.label}</span>
                  <span className="text-muted-foreground text-xs font-mono break-all">
                    {item.value}
                  </span>
                  {item.hint ? (
                    <span className="text-muted-foreground text-[11px]">
                      {item.hint}
                    </span>
                  ) : null}
                </AutocompletePrimitive.Item>
              )}
            </AutocompletePrimitive.List>
          </AutocompletePrimitive.Popup>
        </AutocompletePrimitive.Positioner>
      </AutocompletePrimitive.Portal>
    </AutocompletePrimitive.Root>
  )
})
