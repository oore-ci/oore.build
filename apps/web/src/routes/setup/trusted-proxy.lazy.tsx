import { createLazyFileRoute, useNavigate } from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'
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
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import {
  useSetupStatus,
  useSetupTrustedProxyConfigure,
} from '@/hooks/use-setup'
import { getApiErrorMessage } from '@/lib/api'
import { PageMeta } from '@/lib/seo'
import { loadTrustedProxySetupPrefill } from '@/lib/setup-prefill'
import { useSetupStore } from '@/stores/setup-store'
import { useSetupModeGuard } from '@/hooks/use-setup-route-transitions'
import { SetupStepError } from '@/components/setup-route-components'

const trustedProxyPresetSchema = z.enum(['generic', 'warpgate', 'custom'])
type TrustedProxyPreset = z.infer<typeof trustedProxyPresetSchema>

const presetHeaders: Record<Exclude<TrustedProxyPreset, 'custom'>, string> = {
  generic: 'x-oore-user-email',
  warpgate: 'x-warpgate-username',
}

const trustedProxySchema = z.object({
  proxyPreset: trustedProxyPresetSchema,
  ownerEmail: z.email('Enter a valid owner email'),
  userEmailHeader: z.string().min(1, 'Header name is required'),
  trustedProxyCidrs: z.string().optional(),
  sharedSecret: z.string().optional(),
})

type TrustedProxyForm = z.infer<typeof trustedProxySchema>

export const Route = createLazyFileRoute('/setup/trusted-proxy')({
  component: SetupTrustedProxyStep,
  errorComponent: SetupStepError,
})

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

function headerForPreset(preset: TrustedProxyPreset): string | undefined {
  return preset === 'custom' ? undefined : presetHeaders[preset]
}

function SetupTrustedProxyStep() {
  const navigate = useNavigate()
  const sessionToken = useSetupStore((s) => s.sessionToken)
  const setupInstanceId = useSetupStore((s) => s.instanceId)
  const configureMutation = useSetupTrustedProxyConfigure()
  const { data: status } = useSetupStatus()
  const prefill = loadTrustedProxySetupPrefill(setupInstanceId)
  const prefillPreset = prefill?.proxyPreset ?? 'generic'
  const prefillHeader =
    prefill?.userEmailHeader ??
    headerForPreset(prefillPreset) ??
    presetHeaders.generic

  const form = useForm<TrustedProxyForm>({
    resolver: zodResolver(trustedProxySchema),
    defaultValues: {
      proxyPreset: prefillPreset,
      ownerEmail: prefill?.ownerEmail ?? '',
      userEmailHeader: prefillHeader,
      trustedProxyCidrs: '',
      sharedSecret: '',
    },
    mode: 'onBlur',
  })

  useSetupModeGuard(status, 'trusted_proxy')

  const errorMessage = configureMutation.error
    ? getApiErrorMessage(configureMutation.error, {
        invalid_input:
          'Trusted proxy settings are invalid. Check owner email, header name, and CIDR values.',
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
        setupOwnerEmail: values.ownerEmail.trim().toLowerCase(),
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
          Configure how Oore reads identity headers forwarded by your
          authentication proxy.
        </p>
      </div>

      <Form {...form}>
        <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
          <FormField
            control={form.control}
            name="ownerEmail"
            render={({ field }) => (
              <FormItem>
                <FormLabel>Initial owner email</FormLabel>
                <FormControl>
                  <Input
                    {...field}
                    type="email"
                    placeholder="owner@example.com"
                    autoComplete="email"
                    disabled={configureMutation.isPending}
                  />
                </FormControl>
                <p className="text-xs text-muted-foreground">
                  The first owner claim must arrive from this same
                  proxy-authenticated email.
                </p>
                <FormMessage />
              </FormItem>
            )}
          />

          <FormField
            control={form.control}
            name="proxyPreset"
            render={({ field }) => (
              <FormItem>
                <FormLabel>Proxy preset</FormLabel>
                <FormControl>
                  <Select
                    value={field.value}
                    onValueChange={(value) => {
                      const preset = value as TrustedProxyPreset
                      field.onChange(preset)
                      const header = headerForPreset(preset)
                      if (header) {
                        form.setValue('userEmailHeader', header, {
                          shouldDirty: true,
                          shouldValidate: true,
                        })
                      }
                    }}
                    disabled={configureMutation.isPending}
                  >
                    <SelectTrigger className="w-full">
                      <SelectValue placeholder="Choose proxy" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="generic">Generic proxy</SelectItem>
                      <SelectItem value="warpgate">Warpgate</SelectItem>
                      <SelectItem value="custom">Custom header</SelectItem>
                    </SelectContent>
                  </Select>
                </FormControl>
                <p className="text-xs text-muted-foreground">
                  Warpgate uses <code>x-warpgate-username</code>. Generic uses{' '}
                  <code>x-oore-user-email</code>.
                </p>
                <FormMessage />
              </FormItem>
            )}
          />

          <FormField
            control={form.control}
            name="userEmailHeader"
            render={({ field }) => (
              <FormItem>
                <FormLabel>User email header</FormLabel>
                <FormControl>
                  <Input
                    {...field}
                    placeholder="x-oore-user-email"
                    onChange={(event) => {
                      const nextHeader = event.target.value
                      field.onChange(nextHeader)
                      const currentPreset = form.getValues('proxyPreset')
                      const presetHeader = headerForPreset(currentPreset)
                      if (
                        presetHeader &&
                        nextHeader.trim().toLowerCase() !== presetHeader
                      ) {
                        form.setValue('proxyPreset', 'custom', {
                          shouldDirty: true,
                          shouldValidate: true,
                        })
                      }
                    }}
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
                  Leave empty when the proxy reaches oored over loopback.
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
            {configureMutation.isPending ? 'Saving...' : 'Continue to owner'}
          </Button>
        </form>
      </Form>
    </div>
  )
}
