import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { AlertDialogAction } from './alert-dialog'
import { Button } from './button'

describe('Oore shadcn registry extensions', () => {
  it('keeps confirmation actions destructive by default', () => {
    render(<AlertDialogAction>Delete</AlertDialogAction>)

    expect(screen.getByRole('button', { name: 'Delete' }).className).toContain(
      'cn-button-variant-destructive',
    )
  })

  it('exposes button size for shared touch-target rules', () => {
    render(<Button size="icon-sm">Open</Button>)

    expect(screen.getByRole('button', { name: 'Open' }).dataset.size).toBe(
      'icon-sm',
    )
  })
})
