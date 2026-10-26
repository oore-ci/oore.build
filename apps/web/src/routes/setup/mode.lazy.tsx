import { createLazyFileRoute, useNavigate } from '@tanstack/react-router'
import { useEffect } from 'react'
import { useForm, useWatch } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import {
  Item,
  ItemContent,
  ItemDescription,
  ItemMedia,
  ItemTitle,
} from '@/components/ui/item'
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group'
import { useSetupPreferences, useSetupStatus } from '@/hooks/use-setup'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { PageMeta } from '@/lib/seo'
import { useSetupStore } from '@/stores/setup-store'
import { SetupStepError } from '@/components/setup-route-components'
import { Text } from '@/components/layout/text'

const modeSchema = z.object({
  mode: z.enum(['local', 'remote_oidc', 'remote_trusted']),
})

type ModeForm = z.infer<typeof modeSchema>

const MODE_COMPARISON: Array<{
  value: ModeForm['mode']
  label: string
  description: string
  technicalDetail: string
  terminalOnly?: boolean
}> = [
  {
    value: 'local',
    label: 'Use Oore on this Mac',
    description: 'For one person on this Mac, including through an SSH tunnel.',
    technicalDetail: 'Passwordless · loopback only',
  },
  {
    value: 'remote_oidc',
    label: 'Let people sign in with an identity provider',
    description:
      'For a team using Google, Microsoft Entra ID, Okta, Auth0, or another OIDC provider.',
    technicalDetail: 'Team sign-in · HTTPS required',
  },
  {
    value: 'remote_trusted',
    label: 'Use an existing trusted access proxy',
    description:
      'For Cloudflare Access, oauth2-proxy, or a similar proxy. Continue setup in the terminal.',
    technicalDetail: 'Advanced proxy setup · finished in the terminal',
    terminalOnly: true,
  },
]

export const Route = createLazyFileRoute('/setup/mode')({
  component: SetupModeStep,
  errorComponent: SetupStepError,
})

function toModeValue(
  runtimeMode: 'local' | 'remote' | undefined,
  remoteAuthMode: 'oidc' | 'trusted_proxy' | undefined,
): ModeForm['mode'] {
  if (runtimeMode === 'local') return 'local'
  if (remoteAuthMode === 'trusted_proxy') return 'remote_trusted'
  return 'remote_oidc'
}

function SetupModeStep() {
  const navigate = useNavigate()
  const sessionToken = useSetupStore((s) => s.sessionToken)
  const { data: status } = useSetupStatus()
  const setupModeMutation = useSetupPreferences()

  useEffect(() => {
    if (
      status &&
      !status.is_configured &&
      status.runtime_mode === 'remote' &&
      status.remote_auth_mode === 'trusted_proxy'
    ) {
      void navigate({
        to: '/setup/trusted-proxy',
        viewTransition: { types: ['setup-forward'] },
      })
    }
  }, [navigate, status])

  const modeValues = status
    ? { mode: toModeValue(status.runtime_mode, status.remote_auth_mode) }
    : undefined

  const form = useForm<ModeForm>({
    resolver: zodResolver(modeSchema),
    defaultValues: {
      mode: toModeValue(status?.runtime_mode, status?.remote_auth_mode),
    },
    values: modeValues,
    mode: 'onBlur',
  })
  const selectedMode = useWatch({ control: form.control, name: 'mode' })

  const errorMessage = setupModeMutation.error
    ? getApiErrorMessage(setupModeMutation.error, {
        invalid_state:
          'This access choice cannot change after you create the owner account.',
        session_expired:
          'Your setup session has expired. Restart setup with a fresh bootstrap token.',
        invalid_session:
          'Your setup session is invalid. Restart setup from the token step.',
      })
    : null

  function onSubmit(values: ModeForm) {
    if (!sessionToken) return

    const runtimeMode = values.mode === 'local' ? 'local' : 'remote'
    const remoteAuthMode =
      values.mode === 'remote_trusted'
        ? 'trusted_proxy'
        : values.mode === 'remote_oidc'
          ? 'oidc'
          : undefined

    setupModeMutation.mutate(
      {
        sessionToken,
        runtimeMode,
        remoteAuthMode,
      },
      {
        onSuccess: () => {
          if (values.mode === 'local') {
            void navigate({
              to: '/setup/owner',
              viewTransition: { types: ['setup-forward'] },
            })
            return
          }
          if (values.mode === 'remote_trusted') {
            void navigate({
              to: '/setup/trusted-proxy',
              viewTransition: { types: ['setup-forward'] },
            })
            return
          }
          void navigate({
            to: '/setup/oidc',
            viewTransition: { types: ['setup-forward'] },
          })
        },
      },
    )
  }

  return (
    <div className="space-y-4">
      <PageMeta title="Choose Access" />
      <div className="space-y-1">
        <h2 className="text-lg font-medium">How will you use Oore?</h2>
        <Text variant="muted">
          Choose who can reach this instance and how they sign in.
        </Text>
      </div>

      <Form {...form}>
        <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
          <FormField
            control={form.control}
            name="mode"
            render={({ field }) => (
              <FormItem>
                <FormLabel className="sr-only">Access choice</FormLabel>
                <FormControl>
                  <RadioGroup
                    value={field.value}
                    onValueChange={(value) => field.onChange(value)}
                    disabled={setupModeMutation.isPending}
                    aria-label="Access choice"
                    className="gap-2"
                  >
                    {MODE_COMPARISON.map((mode) => (
                      <Item
                        key={mode.value}
                        render={<label />}
                        variant="outline"
                        size="sm"
                        className="cursor-pointer items-start has-data-checked:border-primary has-data-checked:bg-accent"
                      >
                        <ItemMedia className="pt-0.5">
                          <RadioGroupItem value={mode.value} />
                        </ItemMedia>
                        <ItemContent>
                          <ItemTitle>
                            {mode.label}
                            {mode.terminalOnly ? (
                              <Badge variant="outline">Terminal handoff</Badge>
                            ) : null}
                          </ItemTitle>
                          <ItemDescription className="line-clamp-none">
                            {mode.description}
                          </ItemDescription>
                          <ItemDescription className="line-clamp-none text-xs">
                            {mode.technicalDetail}
                          </ItemDescription>
                        </ItemContent>
                      </Item>
                    ))}
                  </RadioGroup>
                </FormControl>
                <FormMessage />
              </FormItem>
            )}
          />

          {selectedMode === 'remote_trusted' ? (
            <Alert>
              <AlertTitle>Terminal handoff after this step</AlertTitle>
              <AlertDescription>
                Click Continue first. Wait for the original terminal to say that
                setup is paused. The next page then gives you the command to
                run.
              </AlertDescription>
            </Alert>
          ) : null}

          {errorMessage ? (
            <Alert variant="destructive">
              <AlertTitle>Could not save your access choice</AlertTitle>
              <AlertDescription>{errorMessage}</AlertDescription>
            </Alert>
          ) : null}

          <Button
            type="submit"
            className="w-full"
            disabled={setupModeMutation.isPending}
          >
            {setupModeMutation.isPending ? 'Saving...' : 'Continue'}
          </Button>
        </form>
      </Form>
    </div>
  )
}
