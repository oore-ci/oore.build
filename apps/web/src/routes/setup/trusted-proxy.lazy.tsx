import { createLazyFileRoute } from '@tanstack/react-router'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { useSetupStatus } from '@/hooks/use-setup'
import { PageMeta } from '@/lib/seo'
import { useSetupModeGuard } from '@/hooks/use-setup-route-transitions'
import { SetupStepError } from '@/components/setup-route-components'
import { Text } from '@/components/layout/text'

export const Route = createLazyFileRoute('/setup/trusted-proxy')({
  component: SetupTrustedProxyStep,
  errorComponent: SetupStepError,
})

function SetupTrustedProxyStep() {
  const { data: status } = useSetupStatus()

  useSetupModeGuard(status, 'trusted_proxy')

  return (
    <div className="space-y-4">
      <PageMeta title="Setup Trusted Proxy" />
      <div className="space-y-1">
        <h2 className="text-lg font-medium">
          {status?.is_configured
            ? 'Setup completed'
            : 'Continue in the terminal'}
        </h2>
        <Text variant="muted">
          {status?.is_configured
            ? 'Open Oore through the address managed by your trusted access proxy.'
            : 'The terminal will create the private proofs and verify the proxy settings.'}
        </Text>
      </div>

      {!status?.is_configured ? (
        <Alert>
          <AlertTitle>Wait for the original setup command</AlertTitle>
          <AlertDescription>
            <p>
              Wait until it says that setup is paused. Then run this command on
              the Oore device:
            </p>
            <code className="mt-2 block rounded-md bg-muted px-2 py-1 font-mono text-xs">
              oore setup --interface terminal --access trusted-proxy
            </code>
            <p className="mt-2">
              To choose another access method, run <code>oore setup</code> and
              choose it in the terminal.
            </p>
          </AlertDescription>
        </Alert>
      ) : null}
    </div>
  )
}
