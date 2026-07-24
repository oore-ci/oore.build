import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { addInstanceSchema } from '@/components/add-instance-schema'
import type { AddInstanceForm } from '@/components/add-instance-schema'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Item, ItemContent, ItemMedia, ItemTitle } from '@/components/ui/item'
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { isHostedUiOrigin, isLocalLauncherOrigin } from '@/lib/connectivity'
import { useInstanceStore } from '@/stores/instance-store'
import { DEFAULT_INSTANCE_ICON_KEY, INSTANCE_ICONS } from '@/lib/instance-icons'

interface AddInstanceDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

export default function AddInstanceDialog({
  open,
  onOpenChange,
}: AddInstanceDialogProps) {
  const frontendOrigin =
    typeof window === 'undefined'
      ? 'http://localhost:3000'
      : window.location.origin
  const hostedUi = isHostedUiOrigin(frontendOrigin)
  const localLauncher = isLocalLauncherOrigin(frontendOrigin)
  const addInstance = useInstanceStore((s) => s.addInstance)
  const setActiveInstance = useInstanceStore((s) => s.setActiveInstance)

  const form = useForm<AddInstanceForm>({
    resolver: zodResolver(addInstanceSchema(frontendOrigin)),
    defaultValues: { label: '', url: '', icon: DEFAULT_INSTANCE_ICON_KEY },
    mode: 'onBlur',
  })

  function onSubmit(data: AddInstanceForm) {
    const id = addInstance(data.label.trim(), data.url, data.icon)
    setActiveInstance(id)
    form.reset()
    onOpenChange(false)
  }

  function handleOpenChange(nextOpen: boolean) {
    if (!nextOpen) {
      form.reset()
    }
    onOpenChange(nextOpen)
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Add instance</DialogTitle>
          <DialogDescription>
            {hostedUi
              ? 'Connect to an HTTPS-reachable Oore backend (tunnel or reverse proxy).'
              : localLauncher
                ? 'Leave Backend URL empty to use the local oore-web proxy to your daemon.'
                : 'Connect to an Oore backend. Leave URL empty to use the local dev proxy.'}
          </DialogDescription>
        </DialogHeader>

        <Form {...form}>
          <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
            <FormField
              control={form.control}
              name="label"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Label</FormLabel>
                  <FormControl>
                    <Input
                      type="text"
                      placeholder="My CI Server"
                      autoFocus
                      {...field}
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />

            <FormField
              control={form.control}
              name="url"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>
                    Backend URL {!hostedUi ? '(optional)' : null}
                  </FormLabel>
                  <FormControl>
                    <Input
                      type="text"
                      placeholder="https://ci.example.com"
                      {...field}
                    />
                  </FormControl>
                  {hostedUi ? (
                    <FormDescription>
                      <code>https://ci.oore.build</code> requires an explicit
                      HTTPS backend URL and cannot connect to localhost{' '}
                      <code>http://</code> backends directly.
                    </FormDescription>
                  ) : null}
                  {localLauncher ? (
                    <FormDescription>
                      For local oore-web, keep this empty for localhost daemons
                      to use the built-in proxy.
                    </FormDescription>
                  ) : null}
                  <FormMessage />
                </FormItem>
              )}
            />

            <FormField
              control={form.control}
              name="icon"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Icon</FormLabel>
                  <FormControl>
                    <RadioGroup
                      value={field.value}
                      onValueChange={field.onChange}
                      aria-label="Icon"
                      className="grid grid-cols-2 gap-2 sm:grid-cols-3"
                    >
                      {INSTANCE_ICONS.map((entry) => {
                        const Icon = entry.icon
                        return (
                          <Item
                            key={entry.key}
                            render={<label />}
                            variant="outline"
                            size="xs"
                            className="has-data-checked:border-primary has-data-checked:bg-accent"
                          >
                            <ItemMedia>
                              <RadioGroupItem value={entry.key} />
                              <Icon />
                            </ItemMedia>
                            <ItemContent>
                              <ItemTitle>{entry.label}</ItemTitle>
                            </ItemContent>
                          </Item>
                        )
                      })}
                    </RadioGroup>
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />

            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => handleOpenChange(false)}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={!form.formState.isValid}>
                Add
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  )
}
