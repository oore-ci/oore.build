import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import AddInstanceDialog from './AddInstanceDialog'
import EditInstanceDialog from './EditInstanceDialog'

describe('instance dialogs', () => {
  it('labels add-instance fields and the icon choice group', () => {
    render(<AddInstanceDialog open onOpenChange={vi.fn()} />)

    expect(screen.getByLabelText('Label')).toBeTruthy()
    expect(screen.getByLabelText(/Backend URL/)).toBeTruthy()
    expect(screen.getByRole('group', { name: 'Icon' })).toBeTruthy()
  })

  it('labels edit-instance fields and the icon choice group', () => {
    render(
      <EditInstanceDialog
        instance={{
          id: 'instance-1',
          label: 'Build Mac',
          url: 'https://ci.example.com',
          addedAt: 1,
        }}
        open
        onOpenChange={vi.fn()}
      />,
    )

    expect(screen.getByLabelText('Label')).toBeTruthy()
    expect(screen.getByLabelText(/Backend URL/)).toBeTruthy()
    expect(screen.getByRole('group', { name: 'Icon' })).toBeTruthy()
  })
})
