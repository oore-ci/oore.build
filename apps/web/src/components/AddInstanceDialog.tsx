import { useState } from 'react'
import type { ReactNode } from 'react'
import { useForm, useWatch } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { ArrowDown01Icon, ArrowUpRight01Icon } from '@hugeicons/core-free-icons'
import { getSetupStatus } from '@oore/client/operations'
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
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  Drawer,
  DrawerContent,
  DrawerDescription,
  DrawerFooter,
  DrawerHeader,
  DrawerTitle,
} from '@/components/ui/drawer'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { useIsMobile } from '@/hooks/use-mobile'
import { createWebOoreClient } from '@/lib/api-client/client'
import { isHostedUiOrigin, isLocalLauncherOrigin } from '@/lib/connectivity'
import { resolveInstanceApiBaseUrl } from '@/lib/instance-url'
import { toast } from '@/lib/toast'
import type { Instance } from '@/lib/types'
import { useInstanceStore } from '@/stores/instance-store'
import { useSwitchInstance } from '@/hooks/use-switch-instance'
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

/** Two entries point at the same daemon when their API base URLs match. */
function connectionKey(url: string): string {
  const baseUrl = resolveInstanceApiBaseUrl({ url }) ?? ''
  try {
    const parsed = new URL(baseUrl)
    return `${parsed.origin}${parsed.pathname.replace(/\/+$/, '')}`
  } catch {
    return baseUrl.toLowerCase()
  }
}

function findExistingInstance(
  instances: Record<string, Instance>,
  url: string,
): Instance | undefined {
  const key = connectionKey(url)
  return Object.values(instances).find(
    (instance) => connectionKey(instance.url) === key,
  )
}

async function isOoreReachable(url: string): Promise<boolean> {
  const baseUrl = resolveInstanceApiBaseUrl({ url })
  if (!baseUrl) return false
  try {
    await getSetupStatus({ client: createWebOoreClient({ baseUrl }) })
    return true
  } catch {
    return false
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
  const isMobile = useIsMobile()
  const instances = useInstanceStore((s) => s.instances)
  const addInstance = useInstanceStore((s) => s.addInstance)
  const setActiveInstance = useInstanceStore((s) => s.setActiveInstance)
  const switchInstance = useSwitchInstance()
  const [checking, setChecking] = useState(false)
  const [unreachableUrl, setUnreachableUrl] = useState<string | null>(null)

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
  const watchedUrl = useWatch({ control: form.control, name: 'url' })
  const existing = findExistingInstance(
    instances,
    watchedUrl.replace(/\/+$/, ''),
  )
  const unreachable =
    unreachableUrl !== null && unreachableUrl === watchedUrl.trim()

  function close() {
    form.reset()
    setUnreachableUrl(null)
    onOpenChange(false)
  }

  function connect(data: AddInstanceForm) {
    const label =
      data.label.trim() || (data.url ? 'Team instance' : 'Local Mac')
    const id = addInstance(label, data.url, data.icon)
    if (useInstanceStore.getState().activeInstanceId === id) {
      // The first instance becomes active as it is added.
      setActiveInstance(id)
    } else {
      void switchInstance(id)
    }
    toast.success(`Connected to ${label}`)
    close()
  }

  async function onSubmit(data: AddInstanceForm) {
    if (existing) {
      void switchInstance(existing.id)
      toast.success(`Switched to ${existing.label}`)
      close()
      return
    }

    setChecking(true)
    const reachable = await isOoreReachable(data.url)
    setChecking(false)
    if (!reachable) {
      setUnreachableUrl(watchedUrl.trim())
      return
    }
    connect(data)
  }

  function handleOpenChange(nextOpen: boolean) {
    if (!nextOpen) {
      close()
      return
    }
    onOpenChange(nextOpen)
  }

  const title = 'Connect to Oore'
  const description = hostedUi
    ? 'Enter your team’s HTTPS address. Builds run on its Mac.'
    : localLauncher
      ? 'Leave the address empty to use Oore on this Mac, or enter your team’s address.'
      : 'Enter your Oore address. For local development, leave it empty to use the local connection.'

  const fields = (
    <>
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
                  This hosted page needs an HTTPS address; it cannot connect to
                  local HTTP addresses. If your team uses Cloudflare Access,
                  sign in there first, then return here to connect.
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
                    Sign in to server
                    <HugeiconsIcon
                      icon={ArrowUpRight01Icon}
                      data-icon="inline-end"
                    />
                  </Button>
                ) : (
                  <Button type="button" variant="outline" size="sm" disabled>
                    Sign in to server
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
                Leave empty to connect to Oore on this Mac.
              </FormDescription>
            ) : null}
            <FormMessage />
          </FormItem>
        )}
      />

      <Collapsible>
        <CollapsibleTrigger
          render={
            <Button
              type="button"
              variant="ghost"
              className="group/name-trigger -ml-2.5"
            />
          }
        >
          Name and icon (optional)
          <HugeiconsIcon
            icon={ArrowDown01Icon}
            data-icon="inline-end"
            className="transition-transform group-data-panel-open/name-trigger:rotate-180"
          />
        </CollapsibleTrigger>
        <CollapsibleContent className="space-y-4 pt-4">
          <FormField
            control={form.control}
            name="label"
            render={({ field }) => (
              <FormItem>
                <FormLabel>Name (optional)</FormLabel>
                <FormControl>
                  <Input type="text" placeholder="My CI Server" {...field} />
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
      {existing ? (
        <Alert>
          <AlertDescription>
            This address is already saved as “{existing.label}”.
          </AlertDescription>
        </Alert>
      ) : null}
      {unreachable && !existing ? (
        <Alert variant="destructive">
          <AlertDescription>
            Oore did not answer at {watchedUrl.trim() || frontendOrigin}. Check
            the address and that the daemon is running. If a proxy such as
            Cloudflare Access protects it, sign in there first.
          </AlertDescription>
        </Alert>
      ) : null}
    </>
  )

  const actions = (
    <>
      <Button type="button" variant="outline" onClick={close}>
        Cancel
      </Button>
      {unreachable && !existing ? (
        <Button
          type="button"
          variant="outline"
          onClick={() => connect(form.getValues())}
        >
          Save anyway
        </Button>
      ) : null}
      <Button type="submit" disabled={!form.formState.isValid || checking}>
        {checking
          ? 'Checking…'
          : existing
            ? `Switch to ${existing.label}`
            : unreachable
              ? 'Try again'
              : 'Connect'}
      </Button>
    </>
  )

  function renderForm(content: ReactNode) {
    return (
      <Form {...form}>
        <form
          onSubmit={form.handleSubmit(onSubmit)}
          className="flex min-h-0 flex-1 flex-col"
        >
          {content}
        </form>
      </Form>
    )
  }

  if (isMobile) {
    return (
      <Drawer
        swipeDirection="down"
        showSwipeHandle
        open={open}
        onOpenChange={handleOpenChange}
      >
        <DrawerContent>
          <DrawerHeader>
            <DrawerTitle>{title}</DrawerTitle>
            <DrawerDescription>{description}</DrawerDescription>
          </DrawerHeader>
          {renderForm(
            <>
              <div className="flex-1 scroll-fade space-y-4 overflow-y-auto p-4">
                {fields}
              </div>
              <DrawerFooter>{actions}</DrawerFooter>
            </>,
          )}
        </DrawerContent>
      </Drawer>
    )
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        {renderForm(
          <div className="space-y-6">
            <div className="space-y-4">{fields}</div>
            <DialogFooter>{actions}</DialogFooter>
          </div>,
        )}
      </DialogContent>
    </Dialog>
  )
}
