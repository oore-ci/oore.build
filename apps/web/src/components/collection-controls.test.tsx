import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { SortableTableHead } from './collection-controls'

describe('SortableTableHead', () => {
  it('announces the active direction and requests the opposite direction', () => {
    const onSortChange = vi.fn()

    render(
      <table>
        <thead>
          <tr>
            <SortableTableHead
              sort="name"
              sortKey="name"
              direction="asc"
              onSortChange={onSortChange}
            >
              Project
            </SortableTableHead>
          </tr>
        </thead>
      </table>,
    )

    expect(screen.getByRole('columnheader').getAttribute('aria-sort')).toBe(
      'ascending',
    )
    fireEvent.click(screen.getByRole('button', { name: 'Project' }))
    expect(onSortChange).toHaveBeenCalledWith('name', 'desc')
  })

  it('reports none for an inactive sortable column', () => {
    render(
      <table>
        <thead>
          <tr>
            <SortableTableHead
              sort="updated_at"
              sortKey="name"
              direction="desc"
              onSortChange={vi.fn()}
            >
              Project
            </SortableTableHead>
          </tr>
        </thead>
      </table>,
    )

    expect(screen.getByRole('columnheader').getAttribute('aria-sort')).toBe(
      'none',
    )
  })
})
