import type { UseFormReturn } from 'react-hook-form'
import { useWatch } from 'react-hook-form'
import * as z from 'zod'
import type { SmtpConfig, UpdateSmtpConfig } from '@oore/client/models'
import { Badge } from '@/components/ui/badge'
import { Checkbox } from '@/components/ui/checkbox'
import {
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Textarea } from '@/components/ui/textarea'
import { useProjects } from '@/hooks/use-projects'
import type {
  CreateNotificationChannelRequest,
  NotificationChannel,
  NotificationChannelType,
  UpdateNotificationChannelRequest,
} from '@/lib/notification-channels'

const CHANNEL_TYPES = {
  slack: 'Slack',
  webhook: 'Webhook (Generic HTTP POST)',
  mattermost: 'Mattermost',
  email: 'Email (SMTP)',
} satisfies Record<NotificationChannelType, string>

const ALL_PROJECTS = 'all'

const NOTIFICATION_EVENTS = [
  { value: 'succeeded', label: 'Succeeded' },
  { value: 'failed', label: 'Failed' },
  { value: 'canceled', label: 'Canceled' },
  { value: 'timed_out', label: 'Timed out' },
  { value: 'expired', label: 'Expired' },
  { value: 'runner_offline', label: 'Runner offline' },
  { value: 'release_uploaded', label: 'Release uploaded' },
  { value: 'rollout_step_raised', label: 'Rollout increased' },
  { value: 'rollout_halted', label: 'Rollout halted' },
  { value: 'rollout_completed', label: 'Rollout complete' },
  { value: 'release_approval_needed', label: 'Approval needed' },
  { value: 'release_failed', label: 'Release failed' },
] as const

export const notificationChannelSchema = z.object({
  name: z.string().min(1, 'Name is required').max(200, 'Name too long'),
  channel_type: z.enum(['slack', 'webhook', 'mattermost', 'email']),
  enabled: z.boolean(),
  project_id: z.string(),
  url: z.string().optional(),
  secret: z.string().optional(),
  remove_secret: z.boolean(),
  events: z.array(z.string()),
  smtp_host: z.string().optional(),
  smtp_port: z.string().optional(),
  smtp_username: z.string().optional(),
  smtp_password: z.string().optional(),
  smtp_tls_mode: z.enum(['none', 'start_tls', 'tls']).optional(),
  smtp_from_address: z.string().optional(),
  smtp_recipients: z.string().optional(),
})

export const createNotificationChannelSchema =
  notificationChannelSchema.superRefine((values, context) => {
    if (values.channel_type !== 'email') {
      if (!values.url) {
        context.addIssue({
          code: 'custom',
          message: 'URL is required',
          path: ['url'],
        })
      }
      return
    }

    const requiredFields = [
      ['smtp_host', values.smtp_host, 'SMTP host is required'],
      ['smtp_username', values.smtp_username, 'Username is required'],
      ['smtp_password', values.smtp_password, 'Password is required'],
      [
        'smtp_recipients',
        values.smtp_recipients?.trim(),
        'At least one recipient required',
      ],
    ] as const
    for (const [path, value, message] of requiredFields) {
      if (!value) context.addIssue({ code: 'custom', message, path: [path] })
    }

    const port = Number(values.smtp_port)
    if (!port || port < 1 || port > 65535) {
      context.addIssue({
        code: 'custom',
        message: 'Port must be 1-65535',
        path: ['smtp_port'],
      })
    }
    if (!values.smtp_from_address?.includes('@')) {
      context.addIssue({
        code: 'custom',
        message: 'Valid email address required',
        path: ['smtp_from_address'],
      })
    }
  })

export type NotificationChannelFormValues = z.infer<
  typeof notificationChannelSchema
>

export const notificationChannelDefaults: NotificationChannelFormValues = {
  name: '',
  channel_type: 'slack',
  enabled: true,
  project_id: ALL_PROJECTS,
  url: '',
  secret: '',
  remove_secret: false,
  events: [],
  smtp_host: '',
  smtp_port: '587',
  smtp_username: '',
  smtp_password: '',
  smtp_tls_mode: 'start_tls',
  smtp_from_address: '',
  smtp_recipients: '',
}

export function notificationChannelEditValues(
  channel: NotificationChannel,
): NotificationChannelFormValues {
  return {
    ...notificationChannelDefaults,
    name: channel.name,
    channel_type: channel.channel_type,
    enabled: channel.enabled,
    project_id: channel.project_id ?? ALL_PROJECTS,
    events: channel.events,
    smtp_port: '',
    smtp_tls_mode: undefined,
  }
}

function recipients(value?: string): Array<string> {
  return value
    ? value
        .split(',')
        .map((recipient) => recipient.trim())
        .filter(Boolean)
    : []
}

export function createNotificationChannelRequest(
  values: NotificationChannelFormValues,
): CreateNotificationChannelRequest {
  const request = {
    name: values.name,
    channel_type: values.channel_type,
    events: values.events,
    enabled: values.enabled,
    project_id:
      values.project_id === ALL_PROJECTS ? undefined : values.project_id,
  }
  if (values.channel_type !== 'email') {
    return { ...request, url: values.url, secret: values.secret || undefined }
  }

  const smtp_config: SmtpConfig = {
    host: values.smtp_host ?? '',
    port: Number(values.smtp_port),
    username: values.smtp_username ?? '',
    password: values.smtp_password ?? '',
    tls_mode: values.smtp_tls_mode ?? 'start_tls',
    from_address: values.smtp_from_address ?? '',
    recipients: recipients(values.smtp_recipients),
  }
  return { ...request, smtp_config }
}

export function updateNotificationChannelRequest(
  values: NotificationChannelFormValues,
): UpdateNotificationChannelRequest {
  const request: UpdateNotificationChannelRequest = {
    name: values.name,
    enabled: values.enabled,
    events: values.events,
    project_id: values.project_id === ALL_PROJECTS ? '' : values.project_id,
  }
  if (values.channel_type !== 'email') {
    return {
      ...request,
      url: values.url || undefined,
      secret: values.remove_secret ? '' : values.secret || undefined,
    }
  }

  const smtp_config: UpdateSmtpConfig = {}
  if (values.smtp_host) smtp_config.host = values.smtp_host
  if (values.smtp_port) smtp_config.port = Number(values.smtp_port)
  if (values.smtp_username) smtp_config.username = values.smtp_username
  if (values.smtp_password) smtp_config.password = values.smtp_password
  if (values.smtp_tls_mode) smtp_config.tls_mode = values.smtp_tls_mode
  if (values.smtp_from_address)
    smtp_config.from_address = values.smtp_from_address
  if (values.smtp_recipients?.trim())
    smtp_config.recipients = recipients(values.smtp_recipients)

  return Object.keys(smtp_config).length > 0
    ? { ...request, smtp_config }
    : request
}

export function channelTypeLabel(type: NotificationChannelType): string {
  return type === 'webhook'
    ? 'Webhook'
    : type === 'mattermost'
      ? 'Mattermost'
      : type === 'slack'
        ? 'Slack'
        : 'Email (SMTP)'
}

function ProjectScopeField({
  form,
}: {
  form: UseFormReturn<NotificationChannelFormValues>
}) {
  const projectsQuery = useProjects({ limit: 100 })
  const projects = projectsQuery.data?.projects ?? []
  const items = {
    [ALL_PROJECTS]: 'All projects',
    ...Object.fromEntries(
      projects.map((project) => [project.id, project.name]),
    ),
  }

  return (
    <FormField
      control={form.control}
      name="project_id"
      render={({ field }) => (
        <FormItem>
          <FormLabel>Project</FormLabel>
          <Select
            value={field.value}
            onValueChange={field.onChange}
            items={items}
          >
            <FormControl>
              <SelectTrigger>
                <SelectValue placeholder="All projects" />
              </SelectTrigger>
            </FormControl>
            <SelectContent>
              {Object.entries(items).map(([value, label]) => (
                <SelectItem key={value} value={value}>
                  {label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <FormDescription>
            {field.value === ALL_PROJECTS
              ? 'Receives events from every project, plus runner alerts.'
              : 'Receives build and release events for this project only.'}
          </FormDescription>
          <FormMessage />
        </FormItem>
      )}
    />
  )
}

export function NotificationChannelFormFields({
  channel,
  form,
}: {
  channel?: NotificationChannel
  form: UseFormReturn<NotificationChannelFormValues>
}) {
  const channelType = useWatch({ control: form.control, name: 'channel_type' })
  const removeSecret = useWatch({
    control: form.control,
    name: 'remove_secret',
  })
  const editing = !!channel
  const unchanged = editing ? 'Leave blank to keep existing' : undefined

  return (
    <>
      <FormField
        control={form.control}
        name="name"
        render={({ field }) => (
          <FormItem>
            <FormLabel>Name</FormLabel>
            <FormControl>
              <Input
                placeholder={editing ? undefined : 'e.g. Build Alerts'}
                {...field}
              />
            </FormControl>
            <FormMessage />
          </FormItem>
        )}
      />

      {editing ? (
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
              <FormLabel className="text-sm font-normal">Enabled</FormLabel>
            </FormItem>
          )}
        />
      ) : (
        <FormField
          control={form.control}
          name="channel_type"
          render={({ field }) => (
            <FormItem>
              <FormLabel>Type</FormLabel>
              <Select
                value={field.value}
                onValueChange={field.onChange}
                items={CHANNEL_TYPES}
              >
                <FormControl>
                  <SelectTrigger>
                    <SelectValue placeholder="Select channel type" />
                  </SelectTrigger>
                </FormControl>
                <SelectContent>
                  {Object.entries(CHANNEL_TYPES).map(([value, label]) => (
                    <SelectItem key={value} value={value}>
                      {label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <FormDescription>
                {channelType === 'slack'
                  ? 'Posts Slack messages with a link to the build or release and a mention of the person who triggered it.'
                  : channelType === 'webhook'
                    ? 'Sends a JSON payload with event details to your URL.'
                    : channelType === 'mattermost'
                      ? 'Sends a formatted message to a Mattermost incoming webhook.'
                      : 'Sends HTML email notifications via SMTP.'}
              </FormDescription>
              <FormMessage />
            </FormItem>
          )}
        />
      )}

      <ProjectScopeField form={form} />

      {channelType === 'email' ? (
        <>
          {channel ? (
            <>
              <Badge
                variant={channel.has_smtp_config ? 'secondary' : 'outline'}
              >
                {channel.has_smtp_config ? 'SMTP configured' : 'No SMTP config'}
              </Badge>
              <p className="text-sm text-muted-foreground">
                Leave fields blank to keep existing values. Only fill in fields
                you want to change.
              </p>
            </>
          ) : null}

          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <FormField
              control={form.control}
              name="smtp_host"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>SMTP host</FormLabel>
                  <FormControl>
                    <Input
                      placeholder={unchanged ?? 'smtp.example.com'}
                      {...field}
                    />
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
                      placeholder={unchanged ?? '587'}
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
                    <Input
                      placeholder={unchanged ?? 'user@example.com'}
                      {...field}
                    />
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
                    <Input type="password" placeholder={unchanged} {...field} />
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
                <FormLabel>TLS mode</FormLabel>
                <Select value={field.value} onValueChange={field.onChange}>
                  <FormControl>
                    <SelectTrigger>
                      <SelectValue
                        placeholder={
                          editing ? 'Leave unchanged' : 'Select TLS mode'
                        }
                      />
                    </SelectTrigger>
                  </FormControl>
                  <SelectContent>
                    <SelectItem value="start_tls">
                      STARTTLS (port 587)
                    </SelectItem>
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
                <FormLabel>From address</FormLabel>
                <FormControl>
                  <Input
                    type="email"
                    placeholder={unchanged ?? 'ci@example.com'}
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
                    placeholder={
                      editing
                        ? 'Leave blank to keep existing (comma-separated)'
                        : 'alice@example.com, bob@example.com'
                    }
                    rows={3}
                    {...field}
                  />
                </FormControl>
                <FormDescription>
                  Comma-separated list of email addresses
                  {editing ? '. Leave blank to keep existing recipients.' : '.'}
                </FormDescription>
                <FormMessage />
              </FormItem>
            )}
          />
        </>
      ) : (
        <>
          <FormField
            control={form.control}
            name="url"
            render={({ field }) => (
              <FormItem>
                <FormLabel>
                  {editing
                    ? 'New URL (leave blank to keep existing)'
                    : channelType === 'webhook'
                      ? 'Webhook URL'
                      : 'Incoming webhook URL'}
                </FormLabel>
                <FormControl>
                  <Input
                    type="url"
                    placeholder={
                      editing
                        ? 'https://...'
                        : channelType === 'slack'
                          ? 'https://hooks.slack.com/services/...'
                          : channelType === 'mattermost'
                            ? 'https://mattermost.example.com/hooks/...'
                            : 'https://example.com/webhook'
                    }
                    {...field}
                  />
                </FormControl>
                {channel ? (
                  <FormDescription>
                    {channel.has_url
                      ? 'A URL is currently configured. Enter a new one to replace it.'
                      : 'No URL configured.'}
                  </FormDescription>
                ) : null}
                <FormMessage />
              </FormItem>
            )}
          />

          {channelType === 'webhook' ? (
            <>
              <FormField
                control={form.control}
                name="secret"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>
                      {editing ? 'New HMAC secret' : 'HMAC secret (optional)'}
                    </FormLabel>
                    <FormControl>
                      <Input
                        type="password"
                        placeholder={
                          editing
                            ? undefined
                            : 'Used to sign payloads with X-Oore-Signature header'
                        }
                        disabled={removeSecret}
                        {...field}
                      />
                    </FormControl>
                    <FormDescription>
                      {channel
                        ? channel.has_secret
                          ? removeSecret
                            ? 'The configured secret will be removed when you save.'
                            : 'Leave blank to keep the configured secret.'
                          : 'No HMAC secret configured.'
                        : 'If set, each request includes an X-Oore-Signature header with an HMAC-SHA256 signature.'}
                    </FormDescription>
                    <FormMessage />
                  </FormItem>
                )}
              />
              {channel?.has_secret ? (
                <FormField
                  control={form.control}
                  name="remove_secret"
                  render={({ field }) => (
                    <FormItem className="flex items-start gap-3">
                      <FormControl>
                        <Checkbox
                          checked={field.value}
                          onCheckedChange={field.onChange}
                        />
                      </FormControl>
                      <div className="space-y-0.5">
                        <FormLabel>Remove configured HMAC secret</FormLabel>
                        <FormDescription>
                          Future webhook deliveries will no longer include an
                          HMAC signature.
                        </FormDescription>
                      </div>
                    </FormItem>
                  )}
                />
              ) : null}
            </>
          ) : null}

          {channelType === 'slack' ? (
            <>
              <FormField
                control={form.control}
                name="secret"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>
                      {editing ? 'New bot token' : 'Bot token (optional)'}
                    </FormLabel>
                    <FormControl>
                      <Input
                        type="password"
                        placeholder={editing ? undefined : 'xoxb-...'}
                        autoComplete="off"
                        disabled={removeSecret}
                        {...field}
                      />
                    </FormControl>
                    <FormDescription>
                      {channel?.has_secret
                        ? removeSecret
                          ? 'The bot token will be removed when you save.'
                          : 'Leave blank to keep the configured bot token.'
                        : 'A bot token with the users:read.email scope lets Oore mention the person who triggered a build by matching their sign-in email. Without it, messages name them by email.'}
                    </FormDescription>
                    <FormMessage />
                  </FormItem>
                )}
              />
              {channel?.has_secret ? (
                <FormField
                  control={form.control}
                  name="remove_secret"
                  render={({ field }) => (
                    <FormItem className="flex items-start gap-3">
                      <FormControl>
                        <Checkbox
                          checked={field.value}
                          onCheckedChange={field.onChange}
                        />
                      </FormControl>
                      <div className="space-y-0.5">
                        <FormLabel>Remove configured bot token</FormLabel>
                        <FormDescription>
                          Messages will name people by email instead of
                          mentioning them.
                        </FormDescription>
                      </div>
                    </FormItem>
                  )}
                />
              ) : null}
            </>
          ) : null}
        </>
      )}

      <FormField
        control={form.control}
        name="events"
        render={() => (
          <FormItem>
            <FormLabel>Event filter</FormLabel>
            <FormDescription>
              {editing
                ? 'Leave all unchecked to receive all events.'
                : 'Select which events trigger this channel. Leave all unchecked to receive all events.'}
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
                          onCheckedChange={(checked) =>
                            field.onChange(
                              checked
                                ? [...field.value, event.value]
                                : field.value.filter(
                                    (value) => value !== event.value,
                                  ),
                            )
                          }
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
    </>
  )
}
