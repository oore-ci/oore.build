import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import z from 'zod'
import { toast } from 'sonner'
import { useEffect } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { Delete02Icon, TestTube01Icon } from '@hugeicons/core-free-icons'

import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
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
import {
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Skeleton } from '@/components/ui/skeleton'

export const Route = createFileRoute('/settings/notifications/$channelId')({
  staticData: { breadcrumbLabel: 'Channel' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: NotificationChannelDetailPage,
})

const TERMINAL_EVENTS = [
  { value: 'succeeded', label: 'Succeeded' },
  { value: 'failed', label: 'Failed' },
  { value: 'canceled', label: 'Canceled' },
  { value: 'timed_out', label: 'Timed Out' },
  { value: 'expired', label: 'Expired' },
] as const

const schema = z.object({
  name: z.string().min(1, 'Name is required').max(200),
  enabled: z.boolean(),
  url: z.string().optional(),
  secret: z.string().optional(),
  events: z.array(z.string()),
})

type FormValues = z.infer<typeof schema>

function NotificationChannelDetailPage() {
  const { channelId } = Route.useParams()
  const navigate = useNavigate()

  const { data: channelsData, isLoading } = useNotificationChannels()
  const { data: deliveriesData } = useNotificationDeliveries(channelId)
  const updateMutation = useUpdateNotificationChannel()
  const deleteMutation = useDeleteNotificationChannel()
  const testMutation = useTestNotificationChannel()

  const channel = channelsData?.channels.find((c) => c.id === channelId)
  const deliveries = deliveriesData?.deliveries ?? []

  const form = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: {
      name: '',
      enabled: true,
      url: '',
      secret: '',
      events: [],
    },
    mode: 'onBlur',
  })

  useEffect(() => {
    if (channel) {
      form.reset({
        name: channel.name,
        enabled: channel.enabled,
        url: '',
        secret: '',
        events: channel.events,
      })
    }
  }, [channel, form])

  function onSubmit(values: FormValues) {
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

  if (!channel) {
    return (
      <PageLayout width="wide">
        <PageHeader
          title="Channel Not Found"
          back={{ to: '/settings/notifications', label: 'Notifications' }}
        />
        <p className="text-sm text-muted-foreground">
          This notification channel does not exist or has been deleted.
        </p>
      </PageLayout>
    )
  }

  return (
    <PageLayout width="wide">
      <PageMeta title={`${channel.name} — Notifications`} noindex />
      <PageHeader
        title={channel.name}
        description={`${channel.channel_type === 'mattermost' ? 'Mattermost' : 'Webhook'} notification channel`}
        back={{ to: '/settings/notifications', label: 'Notifications' }}
        actions={
          <div className="flex gap-2">
            <Button
              variant="outline"
              onClick={handleTest}
              disabled={testMutation.isPending}
            >
              <HugeiconsIcon icon={TestTube01Icon} size={16} />
              {testMutation.isPending ? 'Sending...' : 'Test'}
            </Button>
            <AlertDialog>
              <AlertDialogTrigger
                render={
                  <Button variant="outline">
                    <HugeiconsIcon icon={Delete02Icon} size={16} />
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

              <FormField
                control={form.control}
                name="url"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>
                      New URL (leave blank to keep existing)
                    </FormLabel>
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

              <FormField
                control={form.control}
                name="events"
                render={() => (
                  <FormItem>
                    <FormLabel>Event Filter</FormLabel>
                    <FormDescription>
                      Leave all unchecked to receive all terminal events.
                    </FormDescription>
                    <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
                      {TERMINAL_EVENTS.map((event) => (
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
                {updateMutation.isPending ? 'Saving...' : 'Save Changes'}
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
            <p className="py-6 text-sm text-muted-foreground">
              No deliveries yet.
            </p>
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
                            ? 'default'
                            : delivery.status === 'failed'
                              ? 'destructive'
                              : 'secondary'
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
