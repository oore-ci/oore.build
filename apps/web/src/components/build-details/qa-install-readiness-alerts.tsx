import {
  Globe as Globe02Icon,
  Info as InformationCircleIcon,
  Smartphone as SmartPhone01Icon,
} from 'lucide-react'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'

export default function QaInstallReadinessAlerts({
  conditions,
  platform,
  readiness,
}: {
  conditions: {
    desktopIos: boolean
    expired: boolean
    needsSafari: boolean
    wrongPhone: boolean
  }
  platform: 'iOS' | 'Android'
  readiness: { ready: boolean; reason?: string } | null
}) {
  if (readiness && !readiness.ready) {
    return (
      <Alert variant="destructive">
        <InformationCircleIcon />
        <AlertTitle>Not install-ready</AlertTitle>
        <AlertDescription>{readiness.reason}</AlertDescription>
      </Alert>
    )
  }
  if (conditions.expired) {
    return (
      <Alert variant="destructive">
        <InformationCircleIcon />
        <AlertTitle>Artifact expired</AlertTitle>
        <AlertDescription>
          Ask a developer to run a fresh build before installing.
        </AlertDescription>
      </Alert>
    )
  }
  if (conditions.needsSafari) {
    return (
      <Alert>
        <Globe02Icon />
        <AlertTitle>Open this page in Safari</AlertTitle>
        <AlertDescription>
          iOS installation can only start from Safari on this iPhone.
        </AlertDescription>
      </Alert>
    )
  }
  if (conditions.desktopIos) {
    return (
      <Alert>
        <SmartPhone01Icon />
        <AlertTitle>Open this page on the registered iPhone</AlertTitle>
        <AlertDescription>
          Use Safari on a device included in this version’s provisioning
          profile.
        </AlertDescription>
      </Alert>
    )
  }
  if (conditions.wrongPhone) {
    return (
      <Alert>
        <InformationCircleIcon />
        <AlertTitle>Open this page on the right device</AlertTitle>
        <AlertDescription>This version is for {platform}.</AlertDescription>
      </Alert>
    )
  }
  return null
}
