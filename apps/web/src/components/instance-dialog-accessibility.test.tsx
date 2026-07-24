import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import AddInstanceDialog from './AddInstanceDialog'

describe('instance dialogs', () => {
  it('labels add-instance fields and the icon choice group', () => {
    render(<AddInstanceDialog open onOpenChange={vi.fn()} />)

    expect(screen.getByLabelText('Label')).toBeTruthy()
    expect(screen.getByLabelText(/Backend URL/)).toBeTruthy()
    expect(screen.getByRole('radiogroup', { name: 'Icon' })).toBeTruthy()
  })
})
