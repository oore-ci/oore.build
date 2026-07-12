import { act, render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import TerminalLogViewer from '@/components/terminal-log-viewer'
import { groupLogs } from '@/components/terminal-log-viewer/log-model'

describe('TerminalLogViewer', () => {
  it('groups marker-delimited logs while step results remain status truth', () => {
    const grouped = groupLogs(
      [
        {
          sequence: 1,
          content:
            '[oore-step] {"event":"start","name":"Build","command":"bun run build"}',
          stream: 'stdout',
        },
        { sequence: 2, content: 'Compiling', stream: 'stdout' },
        {
          sequence: 3,
          content:
            '[oore-step] {"event":"end","name":"Build","status":"succeeded"}',
          stream: 'stdout',
        },
      ],
      [
        {
          name: 'Build',
          status: 'failed',
          started_at: 1,
          finished_at: 2,
          duration_ms: 1000,
        },
      ],
    )

    expect(grouped.allVisibleLogs.map((log) => log.content)).toEqual([
      'Compiling',
    ])
    expect(grouped.stepGroups[0]).toMatchObject({
      name: 'Build',
      status: 'failed',
      command: 'bun run build',
      durationMs: 1000,
      logs: [{ sequence: 2, content: 'Compiling', stream: 'stdout' }],
    })
  })

  it('shows all logs when completed steps have no log markers', async () => {
    await act(async () => {
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
      await Promise.resolve()
    })

    expect(screen.queryByRole('combobox', { name: 'Build step' })).toBeNull()
    expect(
      screen.getByRole('region', { name: 'Build log output' }),
    ).toBeTruthy()
    expect(
      screen.getByRole('button', { name: 'Download raw logs' }),
    ).toBeTruthy()
  })

  it('does not claim terminal logs are absent while they are loading', () => {
    render(
      <TerminalLogViewer
        logs={[]}
        stepResults={[]}
        isStreaming={false}
        isLoading
        isTerminal
      />,
    )

    expect(screen.getByText('Loading build logs...')).toBeTruthy()
    expect(
      screen.queryByText('This build completed without recorded logs.'),
    ).toBeNull()
  })
})
