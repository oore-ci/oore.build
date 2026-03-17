import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import z from 'zod'
import { toast } from 'sonner'

import type { NotificationChannelType } from '@/lib/types'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { useCreateNotificationChannel } from '@/hooks/use-notification-channels'
import { PageMeta } from '@/lib/seo'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { getApiErrorMessage } from '@/lib/api'

export const Route = createFileRoute('/settings/notifications/new')({
  staticData: { breadcrumbLabel: 'New Channel' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: NewNotificationChannelPage,
})

const CHANNEL_TYPES: Record<string, string> = {
  webhook: 'Webhook (Generic HTTP POST)',
  mattermost: 'Mattermost / Slack',
}

const TERMINAL_EVENTS = [
  { value: 'succeeded', label: 'Succeeded' },
  { value: 'failed', label: 'Failed' },
  { value: 'canceled', label: 'Canceled' },
  { value: 'timed_out', label: 'Timed Out' },
  { value: 'expired', label: 'Expired' },
] as const

const schema = z.object({
  name: z.string().min(1, 'Name is required').max(200, 'Name too long'),
  channel_type: z.enum(['webhook', 'mattermost']),
  url: z.string().url('Please enter a valid URL'),
  secret: z.string().optional(),
  events: z.array(z.string()),
})

type FormValues = z.infer<typeof schema>

function NewNotificationChannelPage() {
  const navigate = useNavigate()
  const createMutation = useCreateNotificationChannel()

  const form = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: {
      name: '',
      channel_type: 'webhook',
      url: '',
      secret: '',
      events: [],
    },
    mode: 'onBlur',
  })

  const channelType = form.watch('channel_type')

  function onSubmit(values: FormValues) {
    createMutation.mutate(
      {
        name: values.name,
        channel_type: values.channel_type as NotificationChannelType,
        url: values.url,
        secret: values.secret || undefined,
        events: values.events,
        enabled: true,
      },
      {
        onSuccess: () => {
          toast.success('Notification channel created')
          void navigate({ to: '/settings/notifications' })
        },
        onError: (err) => {
          toast.error(getApiErrorMessage(err, {}))
        },
      },
    )
  }

  return (
    <PageLayout width="wide">
      <PageMeta title="New Notification Channel" noindex />
      <PageHeader
        title="New Notification Channel"
        description="Configure a webhook or Mattermost channel to receive build notifications."
        back={{ to: '/settings/notifications', label: 'Notifications' }}
      />

      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
            Channel Configuration
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
                      <Input placeholder="e.g. Build Alerts" {...field} />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <FormField
                control={form.control}
                name="channel_type"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Type</FormLabel>
                    <Select
                      value={field.value}
                      onValueChange={(value) => field.onChange(value)}
                      items={CHANNEL_TYPES}
                    >
                      <FormControl>
                        <SelectTrigger>
                          <SelectValue placeholder="Select channel type" />
                        </SelectTrigger>
                      </FormControl>
                      <SelectContent>
                        {Object.entries(CHANNEL_TYPES).map(([key, label]) => (
                          <SelectItem key={key} value={key}>
                            {label}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                    <FormDescription>
                      {channelType === 'webhook'
                        ? 'Sends a JSON payload with build details to your URL.'
                        : 'Sends a formatted message to a Mattermost or Slack incoming webhook.'}
                    </FormDescription>
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
                      {channelType === 'mattermost'
                        ? 'Incoming Webhook URL'
                        : 'Webhook URL'}
                    </FormLabel>
                    <FormControl>
                      <Input
                        type="url"
                        placeholder={
                          channelType === 'mattermost'
                            ? 'https://mattermost.example.com/hooks/...'
                            : 'https://example.com/webhook'
                        }
                        {...field}
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              {channelType === 'webhook' ? (
                <FormField
                  control={form.control}
                  name="secret"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>HMAC Secret (optional)</FormLabel>
                      <FormControl>
                        <Input
                          type="password"
                          placeholder="Used to sign payloads with X-Oore-Signature header"
                          {...field}
                        />
                      </FormControl>
                      <FormDescription>
                        If set, each request includes an{' '}
                        <code className="text-xs">X-Oore-Signature</code> header
                        with an HMAC-SHA256 signature.
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
                      Select which build events trigger this channel. Leave all
                      unchecked to receive all terminal events.
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

              <div className="flex gap-2 pt-2">
                <Button type="submit" disabled={createMutation.isPending}>
                  {createMutation.isPending ? 'Creating...' : 'Create Channel'}
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  onClick={() =>
                    void navigate({ to: '/settings/notifications' })
                  }
                >
                  Cancel
                </Button>
              </div>
            </form>
          </Form>
        </CardContent>
      </Card>
    </PageLayout>
  )
}
