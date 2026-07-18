import { HugeiconsIcon } from '@hugeicons/react'
import { InformationCircleIcon } from '@hugeicons/core-free-icons'

import { isTerminalStatus } from '@/hooks/use-builds'
import type { BuildStatus } from '@/lib/types'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'

export default function QaArtifactUnavailableAlert({
  buildStatus,
}: {
  buildStatus: BuildStatus
}) {
  return (
    <Alert variant={buildStatus === 'failed' ? 'destructive' : 'default'}>
      <HugeiconsIcon icon={InformationCircleIcon} />
      <AlertTitle>
        {buildStatus === 'failed'
          ? 'Build failed'
          : isTerminalStatus(buildStatus)
            ? 'No installable artifact'
            : 'Build is still in progress'}
      </AlertTitle>
      <AlertDescription>
        {buildStatus === 'failed'
          ? 'Open Logs for diagnostic output from this build.'
          : isTerminalStatus(buildStatus)
            ? 'This build finished without an installable APK or IPA.'
            : 'Installation becomes available here when a signed APK or IPA is ready.'}
      </AlertDescription>
    </Alert>
  )
}
