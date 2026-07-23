import type { SubmitHandler, UseFormReturn } from 'react-hook-form'
import type { ExternalAccessNetworkFormValues } from '@/routes/settings/preferences'
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
import { Textarea } from '@/components/ui/textarea'

export default function ExternalAccessNetworkDialog({
  form,
  isOwner,
  isPending,
  onOpenChange,
  onSubmit,
  open,
}: {
  form: UseFormReturn<ExternalAccessNetworkFormValues>
  isOwner: boolean
  isPending: boolean
  onOpenChange: (open: boolean) => void
  onSubmit: SubmitHandler<ExternalAccessNetworkFormValues>
  open: boolean
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>External Access network settings</DialogTitle>
          <DialogDescription>
            Configure the public endpoint and allowed frontend origins.
          </DialogDescription>
        </DialogHeader>

        <Form {...form}>
          <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
            <FormField
              control={form.control}
              name="public_url"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Public URL (HTTPS)</FormLabel>
                  <FormControl>
                    <Input
                      type="url"
                      placeholder="https://ci.example.com"
                      {...field}
                      disabled={isPending || !isOwner}
                    />
                  </FormControl>
                  <FormDescription>
                    Must be non-loopback and HTTPS.
                  </FormDescription>
                  <FormMessage />
                </FormItem>
              )}
            />

            <FormField
              control={form.control}
              name="allowed_origins"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Allowed frontend origins</FormLabel>
                  <FormControl>
                    <Textarea
                      rows={5}
                      placeholder="http://localhost:3000&#10;http://127.0.0.1:3000&#10;https://ci.example.com"
                      {...field}
                      disabled={isPending || !isOwner}
                    />
                  </FormControl>
                  <FormDescription>
                    One origin per line (or comma-separated).
                  </FormDescription>
                  <FormMessage />
                </FormItem>
              )}
            />

            <FormField
              control={form.control}
              name="artifact_delivery_url"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Artifact delivery URL (optional)</FormLabel>
                  <FormControl>
                    <Input
                      type="url"
                      placeholder="https://install.ci.example.com"
                      {...field}
                      disabled={isPending || !isOwner}
                    />
                  </FormControl>
                  <FormDescription>
                    Use a separate HTTPS origin when an interactive auth proxy
                    protects the main URL. Expose only Oore’s token-protected
                    artifact delivery paths there.
                  </FormDescription>
                  <FormMessage />
                </FormItem>
              )}
            />

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
                  'Save network settings'
                )}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  )
}
