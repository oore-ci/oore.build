import { toast } from '@/lib/toast'
import {
  CircleAlert as AlertCircleIcon,
  CircleCheck as CheckmarkCircle02Icon,
} from 'lucide-react'
import type { SubmitHandler, UseFormReturn } from 'react-hook-form'
import type { useTestOidcConnection } from '@/hooks/use-artifact-storage'
import type { GetExternalAccessOidcResponse } from '@/lib/types'
import type { ExternalAccessOidcFormValues } from '@/routes/settings/preferences'
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
  form,
  isOwner,
  isSaving,
  oidcConfig,
  onOpenChange,
  onSubmit,
  open,
  testMutation,
}: {
  form: UseFormReturn<ExternalAccessOidcFormValues>
  isOwner: boolean
  isSaving: boolean
  oidcConfig: GetExternalAccessOidcResponse | undefined
  onOpenChange: (open: boolean) => void
  onSubmit: SubmitHandler<ExternalAccessOidcFormValues>
  open: boolean
  testMutation: ReturnType<typeof useTestOidcConnection>
}) {
  return (
    <Dialog
      open={open}
      onOpenChange={(open) => {
        onOpenChange(open)
        if (!open) testMutation.reset()
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

        <Form {...form}>
          <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
            <FormField
              control={form.control}
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
                      disabled={isSaving}
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
              control={form.control}
              name="client_id"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Client ID</FormLabel>
                  <FormControl>
                    <Input
                      placeholder="your-client-id"
                      {...field}
                      disabled={isSaving}
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />

            <FormField
              control={form.control}
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
                      disabled={isSaving}
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

            {testMutation.isSuccess ? (
              <Alert>
                <CheckmarkCircle02Icon size={16} className="text-success" />
                <AlertDescription>
                  Connection successful.{' '}
                  <span className="font-mono text-xs">
                    {testMutation.data.discovered_issuer}
                  </span>
                </AlertDescription>
              </Alert>
            ) : testMutation.isError ? (
              <Alert variant="destructive">
                <AlertCircleIcon size={16} />
                <AlertDescription>
                  Connection failed. Verify the issuer URL and try again.
                </AlertDescription>
              </Alert>
            ) : null}

            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => onOpenChange(false)}
                disabled={isSaving}
              >
                Cancel
              </Button>
              <Button
                type="button"
                variant="secondary"
                disabled={
                  !isOwner ||
                  testMutation.isPending ||
                  isSaving ||
                  !form.watch('issuer_url').trim()
                }
                onClick={() => {
                  const issuerUrl = form.getValues('issuer_url').trim()
                  if (issuerUrl) {
                    testMutation.mutate(
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
                {testMutation.isPending ? (
                  <>
                    <Spinner className="size-4" />
                    Testing...
                  </>
                ) : (
                  'Test connection'
                )}
              </Button>
              <Button type="submit" disabled={!isOwner || isSaving}>
                {isSaving ? (
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
