import { createLazyFileRoute, useNavigate } from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import type { UseFormReturn } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import * as z from 'zod'
import { toast } from 'sonner'

import type { SmtpConfig } from '@/lib/types'
import { useCreateNotificationChannel } from '@/hooks/use-notification-channels'
import { PageMeta } from '@/lib/seo'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
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
import { getApiErrorMessage } from '@/lib/api'

export const Route = createLazyFileRoute('/settings/notifications/new')({
  component: NewNotificationChannelPage,
})

const CHANNEL_TYPES: Record<string, string> = {
  webhook: 'Webhook (Generic HTTP POST)',
  mattermost: 'Mattermost / Slack',
  email: 'Email (SMTP)',
}

const NOTIFICATION_EVENTS = [
  { value: 'succeeded', label: 'Succeeded' },
  { value: 'failed', label: 'Failed' },
  { value: 'canceled', label: 'Canceled' },
  { value: 'timed_out', label: 'Timed Out' },
  { value: 'expired', label: 'Expired' },
  { value: 'runner_offline', label: 'Runner Offline' },
] as const

const schema = z.object({
  name: z.string().min(1, 'Name is required').max(200, 'Name too long'),
  channel_type: z.enum(['webhook', 'mattermost', 'email']),
  url: z.string().optional(),
  secret: z.string().optional(),
  events: z.array(z.string()),
  // SMTP fields
  smtp_host: z.string().optional(),
  smtp_port: z.string().optional(),
  smtp_username: z.string().optional(),
  smtp_password: z.string().optional(),
  smtp_tls_mode: z.enum(['none', 'start_tls', 'tls']).optional(),
  smtp_from_address: z.string().optional(),
  smtp_recipients: z.string().optional(),
})

type FormValues = z.infer<typeof schema>

function NotificationChannelFields({
  channelType,
  form,
  isEmail,
}: {
  channelType: FormValues['channel_type']
  form: UseFormReturn<FormValues>
  isEmail: boolean
}) {
  if (!isEmail) {
    return (
      <>
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
                  <code className="text-xs">X-Oore-Signature</code> header with
                  an HMAC-SHA256 signature.
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
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <FormField
          control={form.control}
          name="smtp_host"
          render={({ field }) => (
            <FormItem>
              <FormLabel>SMTP Host</FormLabel>
              <FormControl>
                <Input placeholder="smtp.example.com" {...field} />
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
                <Input type="number" placeholder="587" {...field} />
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
                <Input placeholder="user@example.com" {...field} />
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
                <Input type="password" {...field} />
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
                  <SelectValue placeholder="Select TLS mode" />
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
              <Input type="email" placeholder="ci@example.com" {...field} />
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
                placeholder="alice@example.com, bob@example.com"
                rows={3}
                {...field}
              />
            </FormControl>
            <FormDescription>
              Comma-separated list of email addresses.
            </FormDescription>
            <FormMessage />
          </FormItem>
        )}
      />
    </>
  )
}

function useNewNotificationChannelPageState() {
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
      smtp_host: '',
      smtp_port: '587',
      smtp_username: '',
      smtp_password: '',
      smtp_tls_mode: 'start_tls',
      smtp_from_address: '',
      smtp_recipients: '',
    },
    mode: 'onBlur',
  })

  const channelType = form.watch('channel_type')
  const isEmail = channelType === 'email'

  function onSubmit(values: FormValues) {
    // Client-side conditional validation
    if (!isEmail && !values.url) {
      form.setError('url', { message: 'URL is required' })
      return
    }
    if (isEmail) {
      if (!values.smtp_host) {
        form.setError('smtp_host', { message: 'SMTP host is required' })
        return
      }
      const port = Number(values.smtp_port)
      if (!port || port < 1 || port > 65535) {
        form.setError('smtp_port', { message: 'Port must be 1-65535' })
        return
      }
      if (!values.smtp_username) {
        form.setError('smtp_username', { message: 'Username is required' })
        return
      }
      if (!values.smtp_password) {
        form.setError('smtp_password', { message: 'Password is required' })
        return
      }
      if (!values.smtp_from_address?.includes('@')) {
        form.setError('smtp_from_address', {
          message: 'Valid email address required',
        })
        return
      }
      if (!values.smtp_recipients?.trim()) {
        form.setError('smtp_recipients', {
          message: 'At least one recipient required',
        })
        return
      }
    }

    const base = {
      name: values.name,
      channel_type: values.channel_type,
      events: values.events,
      enabled: true,
    }

    if (isEmail) {
      const recipients = (values.smtp_recipients ?? '')
        .split(',')
        .map((r) => r.trim())
        .filter(Boolean)

      const smtp_config: SmtpConfig = {
        host: values.smtp_host!,
        port: Number(values.smtp_port),
        username: values.smtp_username!,
        password: values.smtp_password!,
        tls_mode: values.smtp_tls_mode ?? 'start_tls',
        from_address: values.smtp_from_address!,
        recipients,
      }

      createMutation.mutate(
        { ...base, smtp_config },
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
    } else {
      createMutation.mutate(
        {
          ...base,
          url: values.url!,
          secret: values.secret || undefined,
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
  }

  return { channelType, createMutation, form, isEmail, navigate, onSubmit }
}

function NewNotificationChannelPage() {
  const pageState = useNewNotificationChannelPageState()
  const { channelType, createMutation, form, isEmail, navigate, onSubmit } =
    pageState

  return (
    <PageLayout width="wide">
      <PageMeta title="New Notification Channel" noindex />
      <PageHeader
        title="New Notification Channel"
        description="Configure a notification channel to receive build and runner status updates."
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
                        : channelType === 'mattermost'
                          ? 'Sends a formatted message to a Mattermost or Slack incoming webhook.'
                          : 'Sends HTML email notifications via SMTP.'}
                    </FormDescription>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <NotificationChannelFields
                channelType={channelType}
                form={form}
                isEmail={isEmail}
              />

              <FormField
                control={form.control}
                name="events"
                render={() => (
                  <FormItem>
                    <FormLabel>Event Filter</FormLabel>
                    <FormDescription>
                      Select which events trigger this channel. Leave all
                      unchecked to receive all events.
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

              <div className="flex gap-2 pt-2">
                <Button type="submit" disabled={createMutation.isPending}>
                  {createMutation.isPending ? 'Creating...' : 'Create channel'}
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
