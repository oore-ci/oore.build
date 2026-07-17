import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Alert02Icon } from '@hugeicons/core-free-icons'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { useAuthStore } from '@/stores/auth-store'
import { useInstanceStore } from '@/stores/instance-store'
import { useUiStore } from '@/stores/ui-store'
import {
  directRunnerTrustNoticeKey,
  shouldShowDirectRunnerTrustNotice,
} from './direct-runner-policy-banner-utils'

export default function DirectRunnerPolicyBanner() {
  const user = useAuthStore((state) => state.user)
  const activeInstanceId = useInstanceStore((state) => state.activeInstanceId)
  const acknowledgements = useUiStore(
    (state) => state.directRunnerTrustNoticeAcknowledgements,
  )
  const acknowledge = useUiStore(
    (state) => state.acknowledgeDirectRunnerTrustNotice,
  )
  const noticeKey =
    activeInstanceId && user
      ? directRunnerTrustNoticeKey(activeInstanceId, user.user_id)
      : null

  if (
    !shouldShowDirectRunnerTrustNotice(
      user?.role,
      noticeKey,
      acknowledgements,
    ) ||
    noticeKey === null
  ) {
    return null
  }

  return (
    <Alert className="rounded-none border-x-0 border-t-0 border-warning/30 bg-warning/10 text-foreground">
      <HugeiconsIcon
        icon={Alert02Icon}
        size={16}
        className="text-warning"
        aria-hidden
      />
      <AlertTitle>Direct runner access</AlertTitle>
      <AlertDescription className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <span>
          Build commands run with the runner account&apos;s macOS permissions.
          Only repositories you allow can run; all others stay blocked.
        </span>
        <span className="ml-auto flex flex-wrap items-center gap-3">
          <Link
            to="/settings/preferences"
            className="inline-flex min-h-11 items-center font-medium sm:min-h-0"
          >
            Runner settings
          </Link>
          <Link
            to="/settings/integrations"
            className="inline-flex min-h-11 items-center font-medium sm:min-h-0"
          >
            Repository access
          </Link>
          <Button
            type="button"
            size="sm"
            variant="outline"
            className="min-h-11 sm:min-h-8"
            onClick={() => acknowledge(noticeKey)}
          >
            Dismiss
          </Button>
        </span>
      </AlertDescription>
    </Alert>
  )
}
