import { toast } from '@/lib/toast'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  AlertCircleIcon,
  CheckmarkCircle02Icon,
} from '@hugeicons/core-free-icons'
import type { PreferencesPageState } from '@/routes/settings/preferences'
import { getApiErrorMessage } from '@/lib/api'
import { OidcIssuerUrlAutocomplete } from '@/components/oidc-issuer-url-autocomplete'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import { Spinner } from '@/components/ui/spinner'

export default function OidcSettingsDialog({
  state,
}: {
  state: PreferencesPageState
}) {
  const {
    configureExternalAccessOidcMutation,
    externalAccessOidcForm,
    isOwner,
    oidcConfig,
    oidcDialogOpen,
    onSubmitExternalAccessOidc,
    setOidcDialogOpen,
    testOidcConnectionMutation,
  } = state
  return (
    <Dialog
      open={oidcDialogOpen}
      onOpenChange={(open) => {
        setOidcDialogOpen(open)
        if (!open) testOidcConnectionMutation.reset()
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {oidcConfig
              ? 'Update OIDC provider'
              : 'Configure OIDC for External Access'}
          </DialogTitle>
          <DialogDescription>
            Owner-only. This updates runtime OIDC settings used by External
            Access sign-in.
            {oidcConfig?.has_client_secret ? (
              <> Leave the secret field empty to keep the existing secret.</>
            ) : null}
          </DialogDescription>
        </DialogHeader>

        <Form {...externalAccessOidcForm}>
          <form
            onSubmit={externalAccessOidcForm.handleSubmit(
              onSubmitExternalAccessOidc,
            )}
            className="space-y-4"
          >
            <FormField
              control={externalAccessOidcForm.control}
              name="issuer_url"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Issuer URL</FormLabel>
                  <FormControl>
                    <OidcIssuerUrlAutocomplete
                      name={field.name}
                      value={field.value}
                      onValueChange={(next) => field.onChange(next)}
                      onBlur={field.onBlur}
                      ref={field.ref}
                      disabled={configureExternalAccessOidcMutation.isPending}
                    />
                  </FormControl>
                  <FormDescription>
                    Pick a common provider or enter a custom issuer URL.
                    Template entries must be edited before saving.
                  </FormDescription>
                  <FormMessage />
                </FormItem>
              )}
            />

            <FormField
              control={externalAccessOidcForm.control}
              name="client_id"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Client ID</FormLabel>
                  <FormControl>
                    <Input
                      placeholder="your-client-id"
                      {...field}
                      disabled={configureExternalAccessOidcMutation.isPending}
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />

            <FormField
              control={externalAccessOidcForm.control}
              name="client_secret"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Client secret (optional)</FormLabel>
                  <FormControl>
                    <Input
                      type="password"
                      placeholder={
                        oidcConfig?.has_client_secret
                          ? 'Leave empty to keep existing secret'
                          : 'Leave empty for public clients'
                      }
                      {...field}
                      disabled={configureExternalAccessOidcMutation.isPending}
                    />
                  </FormControl>
                  <FormDescription>
                    {oidcConfig?.has_client_secret
                      ? 'Leave empty to keep the existing secret. Enter a new value to rotate.'
                      : 'Leave empty when the provider uses a public client.'}
                  </FormDescription>
                  <FormMessage />
                </FormItem>
              )}
            />

            {testOidcConnectionMutation.isSuccess ? (
              <Alert>
                <HugeiconsIcon
                  icon={CheckmarkCircle02Icon}
                  size={16}
                  className="text-success"
                />
                <AlertDescription>
                  Connection successful.{' '}
                  <span className="font-mono text-xs">
                    {testOidcConnectionMutation.data.discovered_issuer}
                  </span>
                </AlertDescription>
              </Alert>
            ) : testOidcConnectionMutation.isError ? (
              <Alert variant="destructive">
                <HugeiconsIcon icon={AlertCircleIcon} size={16} />
                <AlertDescription>
                  Connection failed. Verify the issuer URL and try again.
                </AlertDescription>
              </Alert>
            ) : null}

            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => setOidcDialogOpen(false)}
                disabled={configureExternalAccessOidcMutation.isPending}
              >
                Cancel
              </Button>
              <Button
                type="button"
                variant="secondary"
                disabled={
                  !isOwner ||
                  testOidcConnectionMutation.isPending ||
                  configureExternalAccessOidcMutation.isPending ||
                  !externalAccessOidcForm.watch('issuer_url').trim()
                }
                onClick={() => {
                  const issuerUrl = externalAccessOidcForm
                    .getValues('issuer_url')
                    .trim()
                  if (issuerUrl) {
                    testOidcConnectionMutation.mutate(
                      { issuer_url: issuerUrl },
                      {
                        onError: (error) => {
                          toast.error(
                            getApiErrorMessage(error, {
                              oidc_discovery_failed:
                                'OIDC discovery failed. Verify issuer URL and provider availability.',
                              invalid_input:
                                'Invalid issuer URL. Enter a valid URL.',
                            }),
                          )
                        },
                      },
                    )
                  }
                }}
              >
                {testOidcConnectionMutation.isPending ? (
                  <>
                    <Spinner className="size-4" />
                    Testing...
                  </>
                ) : (
                  'Test connection'
                )}
              </Button>
              <Button
                type="submit"
                disabled={
                  !isOwner || configureExternalAccessOidcMutation.isPending
                }
              >
                {configureExternalAccessOidcMutation.isPending ? (
                  <>
                    <Spinner className="size-4" />
                    Saving...
                  </>
                ) : (
                  'Update OIDC provider'
                )}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  )
}
