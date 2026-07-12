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

    expect(screen.queryByRole('combobox', { name: 'Build step' })).toBeNull()
    expect(
      screen.getByRole('region', { name: 'Build log output' }),
    ).toBeTruthy()
    expect(
      screen.getByRole('button', { name: 'Download raw logs' }),
    ).toBeTruthy()
  })
})
