import { useForm, useWatch } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { ArrowUpRight01Icon } from '@hugeicons/core-free-icons'
import { HugeiconsIcon } from '@hugeicons/react'
import { addInstanceSchema } from '@/components/add-instance-schema'
import type { AddInstanceForm } from '@/components/add-instance-schema'
import { Button } from '@/components/ui/button'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
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
  Drawer,
  DrawerContent,
  DrawerDescription,
  DrawerFooter,
  DrawerHeader,
  DrawerTitle,
} from '@/components/ui/drawer'
import { isHostedUiOrigin, isLocalLauncherOrigin } from '@/lib/connectivity'
import { useInstanceStore } from '@/stores/instance-store'
import { DEFAULT_INSTANCE_ICON_KEY, INSTANCE_ICONS } from '@/lib/instance-icons'

interface AddInstanceDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

function getHttpsBackendUrl(value: string): string | null {
  try {
    const url = new URL(value.trim())
    if (
      url.protocol !== 'https:' ||
      !url.hostname ||
      url.username ||
      url.password
    ) {
      return null
    }
    return new URL('/v1/public/setup-status', url).toString()
  } catch {
    return null
  }
}

export default function AddInstanceDialog({
  open,
  onOpenChange,
}: AddInstanceDialogProps) {
  const frontendOrigin =
    globalThis.window?.location.origin ?? 'http://localhost:3000'
  const hostedUi = isHostedUiOrigin(frontendOrigin)
  const localLauncher = isLocalLauncherOrigin(frontendOrigin)
  const addInstance = useInstanceStore((s) => s.addInstance)
  const setActiveInstance = useInstanceStore((s) => s.setActiveInstance)

  const form = useForm<AddInstanceForm>({
    resolver: zodResolver(addInstanceSchema(frontendOrigin)),
    defaultValues: { label: '', url: '', icon: DEFAULT_INSTANCE_ICON_KEY },
    mode: 'onChange',
  })
  const backendSignInUrl = useWatch({
    control: form.control,
    name: 'url',
    compute: (value) => (hostedUi ? getHttpsBackendUrl(value) : null),
  })

  function onSubmit(data: AddInstanceForm) {
    const id = addInstance(
      data.label.trim() || (data.url ? 'Team instance' : 'Local Mac'),
      data.url,
      data.icon,
    )
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
    <Drawer swipeDirection="right" open={open} onOpenChange={handleOpenChange}>
      <DrawerContent className="data-[swipe-axis=x]:[--drawer-content-width:calc(100%-1rem)] data-[swipe-axis=x]:sm:[--drawer-content-width:28rem]">
        <DrawerHeader>
          <DrawerTitle>Connect to Oore</DrawerTitle>
          <DrawerDescription>
            {hostedUi
              ? 'Enter the HTTPS address your team uses for Oore. Builds run on its Mac, not in this browser.'
              : localLauncher
                ? 'Leave the address empty to use Oore on this Mac, or enter your team’s address.'
                : 'Enter your Oore address. For local development, leave it empty to use the local connection.'}
          </DrawerDescription>
        </DrawerHeader>

        <Form {...form}>
          <form
            onSubmit={form.handleSubmit(onSubmit)}
            className="flex min-h-0 flex-1 flex-col"
          >
            <div className="flex-1 scroll-fade space-y-4 overflow-y-auto p-4">
              <FormField
                control={form.control}
                name="url"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>
                      Oore address {!hostedUi ? '(optional)' : null}
                    </FormLabel>
                    <FormControl>
                      <Input
                        type="url"
                        autoFocus
                        placeholder="https://ci.example.com"
                        {...field}
                      />
                    </FormControl>
                    {hostedUi ? (
                      <div className="space-y-2">
                        <FormDescription>
                          <code>https://ci.oore.build</code> requires an
                          explicit HTTPS backend URL and cannot connect to
                          localhost <code>http://</code> backends directly. If
                          Cloudflare Access protects this URL, open it and
                          finish sign-in first. Then return here and add the
                          instance.
                        </FormDescription>
                        {backendSignInUrl ? (
                          <Button
                            variant="outline"
                            size="sm"
                            render={
                              <a
                                href={backendSignInUrl}
                                target="_blank"
                                rel="noopener noreferrer"
                              />
                            }
                            nativeButton={false}
                          >
                            Open backend sign-in
                            <HugeiconsIcon
                              icon={ArrowUpRight01Icon}
                              data-icon="inline-end"
                            />
                          </Button>
                        ) : (
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled
                          >
                            Open backend sign-in
                            <HugeiconsIcon
                              icon={ArrowUpRight01Icon}
                              data-icon="inline-end"
                            />
                          </Button>
                        )}
                      </div>
                    ) : null}
                    {localLauncher ? (
                      <FormDescription>
                        For local oore-web, keep this empty for localhost
                        daemons to use the built-in proxy.
                      </FormDescription>
                    ) : null}
                    <FormMessage />
                  </FormItem>
                )}
              />

              <Collapsible>
                <CollapsibleTrigger
                  render={<Button type="button" variant="ghost" />}
                >
                  Name and icon (optional)
                </CollapsibleTrigger>
                <CollapsibleContent className="space-y-4 pt-4">
                  <FormField
                    control={form.control}
                    name="label"
                    render={({ field }) => (
                      <FormItem>
                        <FormLabel>Name (optional)</FormLabel>
                        <FormControl>
                          <Input
                            type="text"
                            placeholder="My CI Server"
                            {...field}
                          />
                        </FormControl>
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
                                    <HugeiconsIcon icon={Icon} />
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
                </CollapsibleContent>
              </Collapsible>
            </div>
            <DrawerFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => handleOpenChange(false)}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={!form.formState.isValid}>
                Connect
              </Button>
            </DrawerFooter>
          </form>
        </Form>
      </DrawerContent>
    </Drawer>
  )
}
