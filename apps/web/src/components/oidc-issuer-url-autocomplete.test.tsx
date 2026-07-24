import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { OidcIssuerUrlAutocomplete } from './oidc-issuer-url-autocomplete'

describe('OidcIssuerUrlAutocomplete', () => {
  it('preserves the input id so a visible label can target it', () => {
    render(
      <div>
        <label htmlFor="issuer-url">Issuer URL</label>
        <OidcIssuerUrlAutocomplete
          id="issuer-url"
          value=""
          onValueChange={vi.fn()}
        />
      </div>,
    )

    expect(screen.getByLabelText('Issuer URL')).toBeTruthy()
  })
})
