import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useEffect } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import z from 'zod'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import {
  useSetupStatus,
  useSetupTrustedProxyConfigure,
} from '@/hooks/use-setup'
import { getApiErrorMessage } from '@/lib/api'
import {
  getActiveInstanceOrRedirect,
  requireSetupSessionOrRedirect,
} from '@/lib/instance-context'
import { PageMeta } from '@/lib/seo'
import { useSetupStore } from '@/stores/setup-store'

const trustedProxySchema = z.object({
  userEmailHeader: z.string().min(1, 'Header name is required'),
  trustedProxyCidrs: z.string().optional(),
  sharedSecret: z.string().optional(),
})

type TrustedProxyForm = z.infer<typeof trustedProxySchema>

export const Route = createFileRoute('/setup/trusted-proxy')({
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireSetupSessionOrRedirect(instance.id)
  },
  component: SetupTrustedProxyStep,
  errorComponent: SetupTrustedProxyError,
})

function SetupTrustedProxyError({ error }: { error: Error }) {
  return (
    <div className="space-y-4">
      <Alert variant="destructive">
        <AlertTitle>Something went wrong</AlertTitle>
        <AlertDescription>{error.message}</AlertDescription>
      </Alert>
    </div>
  )
}

function parseCidrs(raw: string | undefined): Array<string> {
  if (!raw) return []
  return raw
    .split(',')
    .map((value) => value.trim())
    .filter((value) => value.length > 0)
}

function generateSharedSecret(): string {
  const bytes = new Uint8Array(24)
  crypto.getRandomValues(bytes)
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join(
    '',
  )
}

function SetupTrustedProxyStep() {
  const navigate = useNavigate()
  const sessionToken = useSetupStore((s) => s.sessionToken)
  const setCurrentStep = useSetupStore((s) => s.setCurrentStep)
  const configureMutation = useSetupTrustedProxyConfigure()
  const { data: status } = useSetupStatus()

  const form = useForm<TrustedProxyForm>({
    resolver: zodResolver(trustedProxySchema),
    defaultValues: {
      userEmailHeader: 'x-warpgate-username',
      trustedProxyCidrs: '',
      sharedSecret: '',
    },
  })

  useEffect(() => {
    setCurrentStep(2)
  }, [setCurrentStep])

  useEffect(() => {
    if (!status) return
    if (
      status.runtime_mode !== 'remote' ||
      status.remote_auth_mode !== 'trusted_proxy'
    ) {
      void navigate({ to: '/setup/mode' })
    }
  }, [status, navigate])

  const errorMessage = configureMutation.error
    ? getApiErrorMessage(configureMutation.error, {
        invalid_input:
          'Trusted proxy settings are invalid. Check header name and CIDR values.',
        mode_restricted:
          'Switch setup mode to Remote (Trusted Proxy) before configuring this step.',
        session_expired:
          'Your setup session has expired. Restart setup with a fresh bootstrap token.',
        invalid_session:
          'Your setup session is invalid. Restart setup from the token step.',
      })
    : null

  function onSubmit(values: TrustedProxyForm) {
    if (!sessionToken) return

    configureMutation.mutate(
      {
        sessionToken,
        userEmailHeader: values.userEmailHeader.trim(),
        trustedProxyCidrs: parseCidrs(values.trustedProxyCidrs),
        sharedSecret: values.sharedSecret?.trim() || undefined,
      },
      {
        onSuccess: () => {
          void navigate({ to: '/setup/owner' })
        },
      },
    )
  }

  return (
    <div className="space-y-4">
      <PageMeta title="Setup Trusted Proxy" />
      <div className="space-y-1">
        <h2 className="text-lg font-medium">Trusted Proxy Configuration</h2>
        <p className="text-sm text-muted-foreground">
          Configure how Oore reads identity headers forwarded by Warpgate.
        </p>
      </div>

      <Form {...form}>
        <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
          <FormField
            control={form.control}
            name="userEmailHeader"
            render={({ field }) => (
              <FormItem>
                <FormLabel>User email header</FormLabel>
                <FormControl>
                  <Input
                    {...field}
                    placeholder="x-warpgate-username"
                    disabled={configureMutation.isPending}
                  />
                </FormControl>
                <FormMessage />
              </FormItem>
            )}
          />

          <FormField
            control={form.control}
            name="trustedProxyCidrs"
            render={({ field }) => (
              <FormItem>
                <FormLabel>Trusted proxy CIDRs (optional)</FormLabel>
                <FormControl>
                  <Input
                    {...field}
                    placeholder="10.0.0.0/24, 100.64.0.0/10"
                    disabled={configureMutation.isPending}
                  />
                </FormControl>
                <p className="text-xs text-muted-foreground">
                  Leave empty when Warpgate runs on the same host as oored
                  (loopback trust).
                </p>
                <FormMessage />
              </FormItem>
            )}
          />

          <FormField
            control={form.control}
            name="sharedSecret"
            render={({ field }) => (
              <FormItem>
                <FormLabel>Optional shared secret</FormLabel>
                <FormControl>
                  <Input
                    {...field}
                    type="password"
                    placeholder="Optional defense-in-depth secret"
                    disabled={configureMutation.isPending}
                  />
                </FormControl>
                <Button
                  type="button"
                  variant="outline"
                  onClick={() =>
                    form.setValue('sharedSecret', generateSharedSecret(), {
                      shouldDirty: true,
                    })
                  }
                  disabled={configureMutation.isPending}
                  className="w-full"
                >
                  Generate random secret
                </Button>
                <p className="text-xs text-muted-foreground">
                  Save this value now. After configuration, the secret is
                  write-only.
                </p>
                <FormMessage />
              </FormItem>
            )}
          />

          {errorMessage ? (
            <Alert variant="destructive">
              <AlertTitle>Failed to configure trusted proxy</AlertTitle>
              <AlertDescription>{errorMessage}</AlertDescription>
            </Alert>
          ) : null}

          <Button
            type="submit"
            className="w-full"
            disabled={configureMutation.isPending}
          >
            {configureMutation.isPending ? 'Saving...' : 'Continue to Owner'}
          </Button>
        </form>
      </Form>
    </div>
  )
}
