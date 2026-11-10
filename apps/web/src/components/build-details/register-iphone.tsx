import { useState } from 'react'

import { Button } from '@/components/ui/button'
import { Spinner } from '@/components/ui/spinner'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import {
  useDeviceRegistration,
  useStartDeviceRegistration,
} from '@/hooks/use-testers'
import { getApiErrorMessage } from '@/lib/api-client/api-error'

/**
 * Lets a tester add their own iPhone to the app's Apple team: a profile
 * reports the UDID back and Oore registers it, so nobody has to copy one.
 */
export function RegisterIphone({ pipelineId }: { pipelineId: string }) {
  const start = useStartDeviceRegistration(pipelineId)
  const [registrationId, setRegistrationId] = useState<string | null>(null)
  const registrationQuery = useDeviceRegistration(registrationId)
  const registration = registrationQuery.data ?? start.data

  if (registration?.state === 'registered') {
    return (
      <Text variant="caption" role="status">
        {registration.device_name ?? 'This iPhone'} is registered. It can
        install builds made from now on; ask for a new build if this one does
        not install.
      </Text>
    )
  }

  if (registration?.state === 'pending' && registration.profile_url) {
    return (
      <Stack gap={2}>
        <Button
          variant="outline"
          className="w-full"
          render={<a href={registration.profile_url} />}
          nativeButton={false}
        >
          Download the profile
        </Button>
        <Text variant="caption" role="status">
          <Spinner className="mr-1 inline size-3" />
          Then open Settings, tap Profile Downloaded and install it. This page
          updates when Oore has your iPhone.
        </Text>
      </Stack>
    )
  }

  return (
    <Stack gap={2}>
      <Button
        variant="outline"
        className="w-full"
        disabled={start.isPending}
        onClick={() =>
          start.mutate(null, {
            onSuccess: (created) => setRegistrationId(created.id),
          })
        }
      >
        {start.isPending ? <Spinner /> : null}
        Register this iPhone
      </Button>
      <Text variant="caption" role={start.error ? 'alert' : undefined}>
        {start.error
          ? getApiErrorMessage(start.error, {})
          : registration?.state === 'failed'
            ? (registration.error ?? 'Registering failed. Try again.')
            : registration?.state === 'expired'
              ? 'That profile expired. Start again.'
              : 'Not registered yet? Oore can add your iPhone to the team.'}
      </Text>
    </Stack>
  )
}
