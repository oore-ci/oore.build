import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { AlertDialogAction } from './alert-dialog'
import { Table } from './table'
import { Button } from './button'

describe('Oore shadcn registry extensions', () => {
  it('keeps confirmation actions destructive by default', () => {
    render(<AlertDialogAction>Delete</AlertDialogAction>)

    expect(screen.getByRole('button', { name: 'Delete' }).className).toContain(
      'cn-button-variant-destructive',
    )
  })

  it('contains wide tables inside the available app width', () => {
    const { container } = render(<Table />)

    const tableContainer = container.querySelector(
      '[data-slot="table-container"]',
    )
    expect(tableContainer?.className).toContain('min-w-0')
    expect(tableContainer?.className).toContain('max-w-full')
  })

  it('exposes button size for shared touch-target rules', () => {
    render(<Button size="icon-sm">Open</Button>)

    expect(screen.getByRole('button', { name: 'Open' }).dataset.size).toBe(
      'icon-sm',
    )
  })
})
