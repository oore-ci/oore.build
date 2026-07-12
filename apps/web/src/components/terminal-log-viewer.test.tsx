import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import TerminalLogViewer from '@/components/terminal-log-viewer'

describe('TerminalLogViewer', () => {
  it('shows all logs when completed steps have no log markers', () => {
    render(
      <TerminalLogViewer
        logs={[{ sequence: 1, content: 'Build output', stream: 'stdout' }]}
        stepResults={[
          {
            name: 'Build Android',
            status: 'succeeded',
            started_at: 1,
            finished_at: 2,
            duration_ms: 1000,
          },
        ]}
        isStreaming={false}
        isTerminal
      />,
    )

    const stepSelect = screen.getByRole('combobox', { name: 'Build step' })
    expect((stepSelect as HTMLSelectElement).value).toBe('all')
  })
})
