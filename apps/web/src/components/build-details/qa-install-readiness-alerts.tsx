import { HugeiconsIcon } from '@hugeicons/react'
import {
  Globe02Icon,
  InformationCircleIcon,
  SmartPhone01Icon,
} from '@hugeicons/core-free-icons'

import { copyInstallPageLink } from '@/lib/artifact-install'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'

export default function QaInstallReadinessAlerts({
  conditions,
  platform,
  readiness,
}: {
  conditions: {
    desktopIos: boolean
    expired: boolean
    inAppBrowser: boolean
    wrongPhone: boolean
  }
  platform: 'iOS' | 'Android'
  readiness: { ready: boolean; reason?: string } | null
}) {
  if (readiness && !readiness.ready) {
    return (
      <Alert variant="destructive">
        <HugeiconsIcon icon={InformationCircleIcon} />
        <AlertTitle>Not install-ready</AlertTitle>
        <AlertDescription>{readiness.reason}</AlertDescription>
      </Alert>
    )
  }
  if (conditions.expired) {
    return (
      <Alert variant="destructive">
        <HugeiconsIcon icon={InformationCircleIcon} />
        <AlertTitle>Artifact expired</AlertTitle>
        <AlertDescription>
          Ask a developer to run a fresh build before installing.
        </AlertDescription>
      </Alert>
    )
  }
  if (conditions.desktopIos) {
    return (
      <Alert>
        <HugeiconsIcon icon={SmartPhone01Icon} />
        <AlertTitle>Open this page on the registered iPhone</AlertTitle>
        <AlertDescription>
          Use a device included in this version’s provisioning profile.
        </AlertDescription>
      </Alert>
    )
  }
  if (conditions.wrongPhone) {
    return (
      <Alert>
        <HugeiconsIcon icon={InformationCircleIcon} />
        <AlertTitle>Open this page on the right device</AlertTitle>
        <AlertDescription>This version is for {platform}.</AlertDescription>
      </Alert>
    )
  }
  if (conditions.inAppBrowser) {
    return (
      <Alert>
        <HugeiconsIcon icon={Globe02Icon} />
        <AlertTitle>This looks like an in-app browser</AlertTitle>
        <AlertDescription className="flex min-w-0 flex-col items-start gap-2">
          <span>
            Tap Install to try anyway. If the iOS install prompt does not
            appear, open this link in Safari or Chrome.
          </span>
          <Button variant="outline" size="xs" onClick={copyInstallPageLink}>
            Copy link
          </Button>
        </AlertDescription>
      </Alert>
    )
  }
  return null
}
