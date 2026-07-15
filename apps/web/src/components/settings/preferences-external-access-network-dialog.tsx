import type { PreferencesPageState } from '@/routes/settings/preferences'
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

export function ExternalAccessNetworkDialog({
  state,
}: {
  state: PreferencesPageState
}) {
  const {
    externalAccessNetworkForm,
    isOwner,
    networkEditorOpen,
    onSubmitExternalAccessNetwork,
    setNetworkEditorOpen,
    updateNetworkSettingsMutation,
  } = state
  return (
    <Dialog open={networkEditorOpen} onOpenChange={setNetworkEditorOpen}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>External Access Network Settings</DialogTitle>
          <DialogDescription>
            Configure the public endpoint and allowed frontend origins.
          </DialogDescription>
        </DialogHeader>

        <Form {...externalAccessNetworkForm}>
          <form
            onSubmit={externalAccessNetworkForm.handleSubmit(
              onSubmitExternalAccessNetwork,
            )}
            className="space-y-4"
          >
            <FormField
              control={externalAccessNetworkForm.control}
              name="public_url"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Public URL (HTTPS)</FormLabel>
                  <FormControl>
                    <Input
                      type="url"
                      placeholder="https://ci.example.com"
                      {...field}
                      disabled={
                        updateNetworkSettingsMutation.isPending || !isOwner
                      }
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
              control={externalAccessNetworkForm.control}
              name="allowed_origins"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Allowed frontend origins</FormLabel>
                  <FormControl>
                    <Textarea
                      rows={5}
                      placeholder="http://localhost:3000&#10;http://127.0.0.1:3000&#10;https://ci.example.com"
                      {...field}
                      disabled={
                        updateNetworkSettingsMutation.isPending || !isOwner
                      }
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
              control={externalAccessNetworkForm.control}
              name="artifact_delivery_url"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Artifact delivery URL (optional)</FormLabel>
                  <FormControl>
                    <Input
                      type="url"
                      placeholder="https://install.ci.example.com"
                      {...field}
                      disabled={
                        updateNetworkSettingsMutation.isPending || !isOwner
                      }
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
                onClick={() => setNetworkEditorOpen(false)}
                disabled={updateNetworkSettingsMutation.isPending}
              >
                Cancel
              </Button>
              <Button
                type="submit"
                disabled={!isOwner || updateNetworkSettingsMutation.isPending}
              >
                {updateNetworkSettingsMutation.isPending ? (
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
