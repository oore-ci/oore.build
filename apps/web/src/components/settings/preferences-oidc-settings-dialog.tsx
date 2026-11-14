import type { SubmitHandler, UseFormReturn } from 'react-hook-form'
import type { GetExternalAccessOidcResponse } from '@oore/client/models'
import type { ExternalAccessOidcFormValues } from '@/routes/settings/general'
import { OidcIssuerUrlAutocomplete } from '@/components/oidc-issuer-url-autocomplete'
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
}: {
  form: UseFormReturn<ExternalAccessOidcFormValues>
  isOwner: boolean
  isSaving: boolean
  oidcConfig: GetExternalAccessOidcResponse | undefined
  onOpenChange: (open: boolean) => void
  onSubmit: SubmitHandler<ExternalAccessOidcFormValues>
  open: boolean
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[calc(100dvh-2rem)] max-w-[calc(100vw-2rem)] overflow-x-hidden overflow-y-auto sm:max-w-md">
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
                      className="w-full"
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

            <DialogFooter>
              <Button type="submit" disabled={!isOwner || isSaving}>
                {isSaving ? (
                  <>
                    <Spinner className="size-4" />
                    Saving...
                  </>
                ) : (
                  'Save changes'
                )}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  )
}
