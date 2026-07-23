import type { SubmitHandler, UseFormReturn } from 'react-hook-form'
import type { TrustedProxySettingsPublic } from '@/lib/types'
import type { TrustedProxyFormValues } from '@/routes/settings/preferences'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
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
import { Textarea } from '@/components/ui/textarea'

export default function TrustedProxySettingsDialog({
  form,
  isOwner,
  isPending,
  onOpenChange,
  onSubmit,
  open,
  settings,
}: {
  form: UseFormReturn<TrustedProxyFormValues>
  isOwner: boolean
  isPending: boolean
  onOpenChange: (open: boolean) => void
  onSubmit: SubmitHandler<TrustedProxyFormValues>
  open: boolean
  settings: TrustedProxySettingsPublic | undefined
}) {
  const userEmailHeader = form.watch('user_email_header')
  const clearWarpgateTicket = form.watch('clear_warpgate_ticket')
  const isWarpgate =
    userEmailHeader.trim().toLowerCase() === 'x-warpgate-username'
  return (
    <Dialog
      open={open}
      onOpenChange={(open) => {
        onOpenChange(open)
        if (!open) {
          form.setValue('shared_secret', '')
          form.setValue('warpgate_ticket', '')
          form.setValue('clear_warpgate_ticket', false)
        }
      }}
    >
      <DialogContent className="max-h-[calc(100dvh-2rem)] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Trusted Proxy identity settings</DialogTitle>
          <DialogDescription>
            Configure the backend trust contract used when an upstream proxy
            provides the signed-in user email.
          </DialogDescription>
        </DialogHeader>

        <Form {...form}>
          <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
            <FormField
              control={form.control}
              name="user_email_header"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>User email header</FormLabel>
                  <FormControl>
                    <Input
                      placeholder="x-oore-user-email"
                      {...field}
                      disabled={isPending}
                    />
                  </FormControl>
                  <FormDescription>
                    Header forwarded by oore-web after the upstream proxy has
                    proven the request.
                  </FormDescription>
                  <FormMessage />
                </FormItem>
              )}
            />

            <FormField
              control={form.control}
              name="trusted_proxy_cidrs"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Trusted proxy peer CIDRs</FormLabel>
                  <FormControl>
                    <Textarea
                      rows={4}
                      placeholder="127.0.0.1/32&#10;10.0.0.10/32"
                      {...field}
                      disabled={isPending}
                    />
                  </FormControl>
                  <FormDescription>
                    One CIDR per line. Leave blank to accept loopback peers
                    only.
                  </FormDescription>
                  <FormMessage />
                </FormItem>
              )}
            />

            <FormField
              control={form.control}
              name="shared_secret"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Shared secret</FormLabel>
                  <FormControl>
                    <Input
                      type="password"
                      placeholder={
                        settings?.has_shared_secret
                          ? 'Leave empty to keep existing secret'
                          : 'Paste shared secret'
                      }
                      {...field}
                      disabled={isPending}
                    />
                  </FormControl>
                  <FormDescription>
                    Leave empty to keep the existing secret. Enter a new value
                    only when rotating it.
                  </FormDescription>
                  <FormMessage />
                </FormItem>
              )}
            />

            {isWarpgate ? (
              <div className="space-y-3 border p-3">
                <FormField
                  control={form.control}
                  name="warpgate_ticket"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>iOS install access ticket</FormLabel>
                      <FormControl>
                        <Input
                          type="password"
                          placeholder={
                            settings?.has_warpgate_ticket
                              ? 'Leave empty to keep existing ticket'
                              : 'Paste Warpgate access ticket'
                          }
                          autoComplete="off"
                          {...field}
                          disabled={isPending || clearWarpgateTicket}
                        />
                      </FormControl>
                      <FormDescription>
                        {settings?.warpgate_ticket_source === 'environment'
                          ? 'Currently supplied by OORE_WARPGATE_TICKET. Saving a value here stores an encrypted override.'
                          : settings?.has_warpgate_ticket
                            ? 'Stored encrypted. Leave empty to keep it; enter a value only to rotate it.'
                            : 'Optional. Lets the iOS installer fetch the manifest and IPA through Warpgate without an interactive login.'}
                      </FormDescription>
                      <FormMessage />
                    </FormItem>
                  )}
                />

                {settings?.warpgate_ticket_source === 'database' ? (
                  <FormField
                    control={form.control}
                    name="clear_warpgate_ticket"
                    render={({ field }) => (
                      <FormItem>
                        <label className="flex items-center gap-2 text-sm font-medium">
                          <FormControl>
                            <Checkbox
                              checked={field.value}
                              onCheckedChange={(checked) =>
                                field.onChange(!!checked)
                              }
                              disabled={isPending}
                            />
                          </FormControl>
                          Remove stored install ticket
                        </label>
                        <FormDescription>
                          Environment configuration will become active if
                          OORE_WARPGATE_TICKET is also set.
                        </FormDescription>
                      </FormItem>
                    )}
                  />
                ) : null}
              </div>
            ) : null}

            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => onOpenChange(false)}
                disabled={isPending}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={!isOwner || isPending}>
                {isPending ? (
                  <>
                    <Spinner className="size-4" />
                    Saving...
                  </>
                ) : (
                  'Save trusted proxy settings'
                )}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  )
}
