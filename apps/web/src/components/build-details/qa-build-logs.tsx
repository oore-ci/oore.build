import { useMemo } from 'react'

import type { Build, BuildLogChunk } from '@/lib/types'
import { isTerminalStatus, useBuildLogs } from '@/hooks/use-builds'
import { useLogStream } from '@/hooks/use-log-stream'
import { mergeBuildLogSnapshots } from '@/lib/log-stream-utils'
import TerminalLogViewer from '@/components/terminal-log-viewer'

export default function QaBuildLogs({ build }: { build: Build }) {
  const isTerminal = isTerminalStatus(build.status)
  const { logs: streamLogs, isStreaming } = useLogStream(build.id, !isTerminal)
  const fullLogsQuery = useBuildLogs(build.id, { enabled: isTerminal })
  const logs: Array<BuildLogChunk> = useMemo(
    () => mergeBuildLogSnapshots(streamLogs, fullLogsQuery.data?.logs ?? []),
    [streamLogs, fullLogsQuery.data?.logs],
  )

  return (
    <TerminalLogViewer
      logs={logs}
      stepResults={build.step_results ?? []}
      isStreaming={isStreaming && !isTerminal}
      isLoading={isTerminal && fullLogsQuery.isLoading}
      logsUnavailable={fullLogsQuery.isError}
      isTerminal={isTerminal}
    />
  )
}
