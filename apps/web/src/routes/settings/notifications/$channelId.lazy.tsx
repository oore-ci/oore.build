import { createLazyFileRoute, useNavigate } from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import type { UseFormReturn } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'
import { toast } from '@/lib/toast'
import { HugeiconsIcon } from '@hugeicons/react'
import { Delete02Icon, TestTube01Icon } from '@hugeicons/core-free-icons'

import type { NotificationChannel, UpdateSmtpConfig } from '@/lib/types'
import {
  useDeleteNotificationChannel,
  useNotificationChannels,
  useNotificationDeliveries,
  useTestNotificationChannel,
  useUpdateNotificationChannel,
} from '@/hooks/use-notification-channels'
import { getApiErrorMessage } from '@/lib/api'
import { PageMeta } from '@/lib/seo'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '@/components/ui/alert-dialog'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import {
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'

export const Route = createLazyFileRoute('/settings/notifications/$channelId')({
  component: NotificationChannelDetailPage,
})

const NOTIFICATION_EVENTS = [
  { value: 'succeeded', label: 'Succeeded' },
  { value: 'failed', label: 'Failed' },
  { value: 'canceled', label: 'Canceled' },
  { value: 'timed_out', label: 'Timed Out' },
  { value: 'expired', label: 'Expired' },
  { value: 'runner_offline', label: 'Runner Offline' },
] as const

const schema = z.object({
  name: z.string().min(1, 'Name is required').max(200),
  enabled: z.boolean(),
  url: z.string().optional(),
  secret: z.string().optional(),
  events: z.array(z.string()),
  // SMTP fields (all optional for edit — blank = keep existing)
  smtp_host: z.string().optional(),
  smtp_port: z.string().optional(),
  smtp_username: z.string().optional(),
  smtp_password: z.string().optional(),
  smtp_tls_mode: z.enum(['none', 'start_tls', 'tls']).optional(),
  smtp_from_address: z.string().optional(),
  smtp_recipients: z.string().optional(),
})

type FormValues = z.infer<typeof schema>

function channelTypeLabel(type: string): string {
  switch (type) {
    case 'webhook':
      return 'Webhook'
    case 'mattermost':
      return 'Mattermost'
    case 'email':
      return 'Email (SMTP)'
    default:
      return type
  }
}

function NotificationChannelSettingsFields({
  channel,
  form,
}: {
  channel: NotificationChannel
  form: UseFormReturn<FormValues>
}) {
  if (channel.channel_type !== 'email') {
    return (
      <>
        <FormField
          control={form.control}
          name="url"
          render={({ field }) => (
            <FormItem>
              <FormLabel>New URL (leave blank to keep existing)</FormLabel>
              <FormControl>
                <Input type="url" placeholder="https://..." {...field} />
              </FormControl>
              <FormDescription>
                {channel.has_url
                  ? 'A URL is currently configured. Enter a new one to replace it.'
                  : 'No URL configured.'}
              </FormDescription>
              <FormMessage />
            </FormItem>
          )}
        />

        {channel.channel_type === 'webhook' ? (
          <FormField
            control={form.control}
            name="secret"
            render={({ field }) => (
              <FormItem>
                <FormLabel>
                  New HMAC Secret (leave blank to keep existing)
                </FormLabel>
                <FormControl>
                  <Input type="password" {...field} />
                </FormControl>
                <FormDescription>
                  {channel.has_secret
                    ? 'A secret is configured. Enter a new one to replace it.'
                    : 'No HMAC secret configured.'}
                </FormDescription>
                <FormMessage />
              </FormItem>
            )}
          />
        ) : null}
      </>
    )
  }

  return (
    <>
      <div className="flex items-center gap-2 text-sm">
        <Badge variant={channel.has_smtp_config ? 'secondary' : 'outline'}>
          {channel.has_smtp_config ? 'SMTP configured' : 'No SMTP config'}
        </Badge>
      </div>

      <FormDescription>
        Leave fields blank to keep existing values. Only fill in fields you want
        to change.
      </FormDescription>

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <FormField
          control={form.control}
          name="smtp_host"
          render={({ field }) => (
            <FormItem>
              <FormLabel>SMTP Host</FormLabel>
              <FormControl>
                <Input placeholder="Leave blank to keep existing" {...field} />
              </FormControl>
              <FormMessage />
            </FormItem>
          )}
        />
        <FormField
          control={form.control}
          name="smtp_port"
          render={({ field }) => (
            <FormItem>
              <FormLabel>Port</FormLabel>
              <FormControl>
                <Input
                  type="number"
                  placeholder="Leave blank to keep existing"
                  {...field}
                />
              </FormControl>
              <FormMessage />
            </FormItem>
          )}
        />
      </div>

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <FormField
          control={form.control}
          name="smtp_username"
          render={({ field }) => (
            <FormItem>
              <FormLabel>Username</FormLabel>
              <FormControl>
                <Input placeholder="Leave blank to keep existing" {...field} />
              </FormControl>
              <FormMessage />
            </FormItem>
          )}
        />
        <FormField
          control={form.control}
          name="smtp_password"
          render={({ field }) => (
            <FormItem>
              <FormLabel>Password</FormLabel>
              <FormControl>
                <Input
                  type="password"
                  placeholder="Leave blank to keep existing"
                  {...field}
                />
              </FormControl>
              <FormMessage />
            </FormItem>
          )}
        />
      </div>

      <FormField
        control={form.control}
        name="smtp_tls_mode"
        render={({ field }) => (
          <FormItem>
            <FormLabel>TLS Mode</FormLabel>
            <Select
              value={field.value}
              onValueChange={(value) => field.onChange(value)}
            >
              <FormControl>
                <SelectTrigger>
                  <SelectValue placeholder="Leave unchanged" />
                </SelectTrigger>
              </FormControl>
              <SelectContent>
                <SelectItem value="start_tls">STARTTLS (port 587)</SelectItem>
                <SelectItem value="tls">Implicit TLS (port 465)</SelectItem>
                <SelectItem value="none">None (unencrypted)</SelectItem>
              </SelectContent>
            </Select>
            <FormMessage />
          </FormItem>
        )}
      />

      <FormField
        control={form.control}
        name="smtp_from_address"
        render={({ field }) => (
          <FormItem>
            <FormLabel>From Address</FormLabel>
            <FormControl>
              <Input
                type="email"
                placeholder="Leave blank to keep existing"
                {...field}
              />
            </FormControl>
            <FormMessage />
          </FormItem>
        )}
      />

      <FormField
        control={form.control}
        name="smtp_recipients"
        render={({ field }) => (
          <FormItem>
            <FormLabel>Recipients</FormLabel>
            <FormControl>
              <Textarea
                placeholder="Leave blank to keep existing (comma-separated)"
                rows={3}
                {...field}
              />
            </FormControl>
            <FormDescription>
              Comma-separated list of email addresses. Leave blank to keep
              existing recipients.
            </FormDescription>
            <FormMessage />
          </FormItem>
        )}
      />
    </>
  )
}

function useNotificationChannelDetailPageState() {
  const { channelId } = Route.useParams()
  const navigate = useNavigate()

  const { data: channelsData, isLoading } = useNotificationChannels()
  const { data: deliveriesData } = useNotificationDeliveries(channelId)
  const updateMutation = useUpdateNotificationChannel()
  const deleteMutation = useDeleteNotificationChannel()
  const testMutation = useTestNotificationChannel()

  const channel = channelsData?.channels.find((c) => c.id === channelId)
  const deliveries = deliveriesData?.deliveries ?? []
  const isEmail = channel?.channel_type === 'email'

  const channelValues = channel
    ? {
        name: channel.name,
        enabled: channel.enabled,
        url: '',
        secret: '',
        events: channel.events,
        smtp_host: '',
        smtp_port: '',
        smtp_username: '',
        smtp_password: '',
        smtp_tls_mode: undefined,
        smtp_from_address: '',
        smtp_recipients: '',
      }
    : undefined

  const form = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: {
      name: '',
      enabled: true,
      url: '',
      secret: '',
      events: [],
      smtp_host: '',
      smtp_port: '',
      smtp_username: '',
      smtp_password: '',
      smtp_tls_mode: undefined,
      smtp_from_address: '',
      smtp_recipients: '',
    },
    values: channelValues,
    mode: 'onBlur',
  })

  function onSubmit(values: FormValues) {
    if (isEmail) {
      // Build partial SMTP config — only include fields with actual values
      const smtp_config: UpdateSmtpConfig = {}
      if (values.smtp_host) smtp_config.host = values.smtp_host
      if (values.smtp_port) smtp_config.port = Number(values.smtp_port)
      if (values.smtp_username) smtp_config.username = values.smtp_username
      if (values.smtp_password) smtp_config.password = values.smtp_password
      if (values.smtp_tls_mode) smtp_config.tls_mode = values.smtp_tls_mode
      if (values.smtp_from_address)
        smtp_config.from_address = values.smtp_from_address
      if (values.smtp_recipients?.trim()) {
        smtp_config.recipients = values.smtp_recipients
          .split(',')
          .map((r) => r.trim())
          .filter(Boolean)
      }

      const hasSmtpChanges = Object.keys(smtp_config).length > 0

      updateMutation.mutate(
        {
          id: channelId,
          data: {
            name: values.name,
            enabled: values.enabled,
            events: values.events,
            ...(hasSmtpChanges ? { smtp_config } : {}),
          },
        },
        {
          onSuccess: () => toast.success('Channel updated'),
          onError: (err) => toast.error(getApiErrorMessage(err, {})),
        },
      )
    } else {
      updateMutation.mutate(
        {
          id: channelId,
          data: {
            name: values.name,
            enabled: values.enabled,
            events: values.events,
            url: values.url || undefined,
            secret: values.secret || undefined,
          },
        },
        {
          onSuccess: () => toast.success('Channel updated'),
          onError: (err) => toast.error(getApiErrorMessage(err, {})),
        },
      )
    }
  }

  function handleDelete() {
    deleteMutation.mutate(channelId, {
      onSuccess: () => {
        toast.success('Channel deleted')
        void navigate({ to: '/settings/notifications' })
      },
      onError: (err) => toast.error(`Failed to delete: ${err.message}`),
    })
  }

  function handleTest() {
    testMutation.mutate(channelId, {
      onSuccess: (result) => {
        if (result.success) {
          toast.success('Test notification sent')
        } else {
          toast.error(`Test failed: ${result.error ?? 'Unknown error'}`)
        }
      },
      onError: (err) => toast.error(`Test failed: ${err.message}`),
    })
  }

  if (isLoading) {
    return { status: 'loading' as const }
  }

  if (!channel) {
    return { status: 'missing' as const }
  }

  return {
    status: 'ready' as const,
    channel,
    deleteMutation,
    deliveries,
    form,
    handleDelete,
    handleTest,
    isEmail,
    onSubmit,
    testMutation,
    updateMutation,
  }
}

function NotificationChannelDetailPage() {
  const pageState = useNotificationChannelDetailPageState()

  if (pageState.status === 'loading') {
    return (
      <PageLayout width="wide">
        <Skeleton className="h-8 w-48" />
        <Card>
          <CardContent className="space-y-3">
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
          </CardContent>
        </Card>
      </PageLayout>
    )
  }

  if (pageState.status === 'missing') {
    return (
      <PageLayout width="wide">
        <PageHeader title="Channel Not Found" />
        <p className="text-sm text-muted-foreground">
          This notification channel does not exist or has been deleted.
        </p>
      </PageLayout>
    )
  }

  const {
    channel,
    deleteMutation,
    deliveries,
    form,
    handleDelete,
    handleTest,
    onSubmit,
    testMutation,
    updateMutation,
  } = pageState

  return (
    <PageLayout width="wide">
      <PageMeta title={`${channel.name} — Notifications`} noindex />
      <PageHeader
        title={channel.name}
        description={`${channelTypeLabel(channel.channel_type)} notification channel`}
        actions={
          <div className="flex gap-2">
            <Button
              variant="outline"
              onClick={handleTest}
              disabled={testMutation.isPending}
            >
              <HugeiconsIcon icon={TestTube01Icon} />
              {testMutation.isPending ? 'Sending...' : 'Test'}
            </Button>
            <AlertDialog>
              <AlertDialogTrigger
                render={
                  <Button variant="destructive">
                    <HugeiconsIcon icon={Delete02Icon} />
                    Delete
                  </Button>
                }
              />
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle>
                    Delete notification channel?
                  </AlertDialogTitle>
                  <AlertDialogDescription>
                    This permanently removes the channel and its delivery
                    history.
                  </AlertDialogDescription>
                </AlertDialogHeader>
                <AlertDialogFooter>
                  <AlertDialogCancel>Cancel</AlertDialogCancel>
                  <AlertDialogAction
                    onClick={handleDelete}
                    disabled={deleteMutation.isPending}
                  >
                    {deleteMutation.isPending ? 'Deleting...' : 'Delete'}
                  </AlertDialogAction>
                </AlertDialogFooter>
              </AlertDialogContent>
            </AlertDialog>
          </div>
        }
      />

      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
            Settings
          </CardTitle>
        </CardHeader>
        <CardContent>
          <Form {...form}>
            <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
              <FormField
                control={form.control}
                name="name"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Name</FormLabel>
                    <FormControl>
                      <Input {...field} />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <FormField
                control={form.control}
                name="enabled"
                render={({ field }) => (
                  <FormItem className="flex items-center gap-2 space-y-0">
                    <FormControl>
                      <Checkbox
                        checked={field.value}
                        onCheckedChange={field.onChange}
                      />
                    </FormControl>
                    <FormLabel className="text-sm font-normal">
                      Enabled
                    </FormLabel>
                  </FormItem>
                )}
              />

              <NotificationChannelSettingsFields
                channel={channel}
                form={form}
              />

              <FormField
                control={form.control}
                name="events"
                render={() => (
                  <FormItem>
                    <FormLabel>Event Filter</FormLabel>
                    <FormDescription>
                      Leave all unchecked to receive all events.
                    </FormDescription>
                    <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
                      {NOTIFICATION_EVENTS.map((event) => (
                        <FormField
                          key={event.value}
                          control={form.control}
                          name="events"
                          render={({ field }) => (
                            <FormItem className="flex items-center gap-2 space-y-0">
                              <FormControl>
                                <Checkbox
                                  checked={field.value.includes(event.value)}
                                  onCheckedChange={(checked) => {
                                    const current = field.value
                                    if (checked) {
                                      field.onChange([...current, event.value])
                                    } else {
                                      field.onChange(
                                        current.filter(
                                          (v: string) => v !== event.value,
                                        ),
                                      )
                                    }
                                  }}
                                />
                              </FormControl>
                              <FormLabel className="text-sm font-normal">
                                {event.label}
                              </FormLabel>
                            </FormItem>
                          )}
                        />
                      ))}
                    </div>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <Button type="submit" disabled={updateMutation.isPending}>
                {updateMutation.isPending ? 'Saving...' : 'Save changes'}
              </Button>
            </form>
          </Form>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
            Delivery History
          </CardTitle>
        </CardHeader>
        <CardContent>
          {deliveries.length === 0 ? (
            <p className="text-sm text-muted-foreground">No deliveries yet.</p>
          ) : (
            <div className="space-y-2">
              {deliveries.map((delivery) => (
                <div
                  key={delivery.id}
                  className="flex items-center justify-between gap-3 border border-border/60 p-3 text-sm"
                >
                  <div className="min-w-0 space-y-1">
                    <div className="flex items-center gap-2">
                      <Badge
                        variant={
                          delivery.status === 'delivered'
                            ? 'secondary'
                            : delivery.status === 'failed'
                              ? 'destructive'
                              : 'outline'
                        }
                      >
                        {delivery.status}
                      </Badge>
                      <span className="text-muted-foreground">
                        {delivery.event_type}
                      </span>
                    </div>
                    {delivery.last_error ? (
                      <p className="truncate text-xs text-destructive">
                        {delivery.last_error}
                      </p>
                    ) : null}
                  </div>
                  <span className="shrink-0 text-xs text-muted-foreground">
                    {new Date(delivery.created_at * 1000).toLocaleString()}
                  </span>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>
    </PageLayout>
  )
}
