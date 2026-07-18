import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { LocalOnlySourcesNotice } from './-local-only-sources-notice'

describe('LocalOnlySourcesNotice', () => {
  it('explains the mode inline without rendering a persistent alert banner', () => {
    render(<LocalOnlySourcesNotice />)

    expect(screen.getByText('Local Only mode')).toBeTruthy()
    expect(screen.queryByRole('alert')).toBeNull()
    expect(screen.queryByText(/External access required/i)).toBeNull()
  })
})
