import type { UseFormReturn } from 'react-hook-form'
import { useWatch } from 'react-hook-form'
import * as z from 'zod'
import type { SmtpConfig, UpdateSmtpConfig } from '@oore/client/models'
import { Checkbox } from '@/components/ui/checkbox'
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group'
import { Switch } from '@/components/ui/switch'
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
import { formatEnumLabel } from '@/lib/format-utils'
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

const TLS_MODES = {
  start_tls: 'STARTTLS (port 587)',
  tls: 'Implicit TLS (port 465)',
  none: 'None (unencrypted)',
} as const

const ALL_PROJECTS = 'all'

const NOTIFICATION_EVENT_GROUPS = [
  {
    label: 'Builds',
    events: [
      { value: 'succeeded', label: 'Build succeeded' },
      { value: 'failed', label: 'Build failed' },
      { value: 'canceled', label: 'Build canceled' },
      { value: 'timed_out', label: 'Build timed out' },
      { value: 'expired', label: 'Build expired in queue' },
    ],
  },
  {
    label: 'Releases',
    events: [
      { value: 'release_uploaded', label: 'Release uploaded' },
      { value: 'release_approval_needed', label: 'Release approval needed' },
      { value: 'rollout_step_raised', label: 'Rollout increased' },
      { value: 'rollout_halted', label: 'Rollout halted' },
      { value: 'rollout_completed', label: 'Rollout complete' },
      { value: 'release_failed', label: 'Release failed' },
    ],
  },
  {
    label: 'Runners',
    events: [{ value: 'runner_offline', label: 'Runner offline' }],
  },
] as const

const NOTIFICATION_EVENT_LABELS = new Map<string, string>(
  NOTIFICATION_EVENT_GROUPS.flatMap((group) =>
    group.events.map((event) => [event.value, event.label] as const),
  ),
)

export function notificationEventLabel(event: string): string {
  return NOTIFICATION_EVENT_LABELS.get(event) ?? formatEnumLabel(event)
}

export const notificationChannelSchema = z.object({
  name: z.string().min(1, 'Name is required').max(200, 'Name too long'),
  channel_type: z.enum(['slack', 'webhook', 'mattermost', 'email']),
  enabled: z.boolean(),
  project_id: z.string(),
  url: z.string().optional(),
  secret: z.string().optional(),
  remove_secret: z.boolean(),
  event_scope: z.enum(['all', 'selected']),
  events: z.array(z.string()),
  smtp_host: z.string().optional(),
  smtp_port: z.string().optional(),
  smtp_username: z.string().optional(),
  smtp_password: z.string().optional(),
  smtp_tls_mode: z.enum(['none', 'start_tls', 'tls']).optional(),
  smtp_from_address: z.string().optional(),
  smtp_recipients: z.string().optional(),
})

type NotificationChannelSchemaValues = z.infer<typeof notificationChannelSchema>

function requireSelectedEvents(
  values: NotificationChannelSchemaValues,
  context: z.RefinementCtx,
) {
  if (values.event_scope === 'selected' && values.events.length === 0) {
    context.addIssue({
      code: 'custom',
      message: 'Select at least one event, or choose All events',
      path: ['events'],
    })
  }
}

// Saved SMTP settings are prefilled when editing, so clearing a required
// field is a mistake rather than a request to keep the saved value.
export function editNotificationChannelSchema(hasSmtpSummary: boolean) {
  return notificationChannelSchema.superRefine((values, context) => {
    requireSelectedEvents(values, context)
    if (values.channel_type !== 'email' || !hasSmtpSummary) return
    const requiredFields = [
      ['smtp_host', values.smtp_host?.trim(), 'SMTP host is required'],
      ['smtp_username', values.smtp_username?.trim(), 'Username is required'],
      [
        'smtp_recipients',
        values.smtp_recipients?.trim(),
        'At least one recipient required',
      ],
    ] as const
    for (const [path, value, message] of requiredFields) {
      if (!value) context.addIssue({ code: 'custom', message, path: [path] })
    }
    if (!values.smtp_from_address?.includes('@')) {
      context.addIssue({
        code: 'custom',
        message: 'Valid email address required',
        path: ['smtp_from_address'],
      })
    }
  })
}

export const createNotificationChannelSchema =
  notificationChannelSchema.superRefine((values, context) => {
    requireSelectedEvents(values, context)
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
  event_scope: 'all',
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
  const smtp = channel.smtp_summary
  return {
    ...notificationChannelDefaults,
    name: channel.name,
    channel_type: channel.channel_type,
    enabled: channel.enabled,
    project_id: channel.project_id ?? ALL_PROJECTS,
    event_scope: channel.events.length > 0 ? 'selected' : 'all',
    events: channel.events,
    smtp_host: smtp?.host ?? '',
    smtp_port: smtp ? String(smtp.port) : '',
    smtp_username: smtp?.username ?? '',
    smtp_tls_mode: smtp?.tls_mode,
    smtp_from_address: smtp?.from_address ?? '',
    smtp_recipients: smtp?.recipients.join(', ') ?? '',
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

// An empty list means every event.
function selectedEvents(values: NotificationChannelFormValues): Array<string> {
  return values.event_scope === 'all' ? [] : values.events
}

export function createNotificationChannelRequest(
  values: NotificationChannelFormValues,
): CreateNotificationChannelRequest {
  const request = {
    name: values.name,
    channel_type: values.channel_type,
    events: selectedEvents(values),
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
    events: selectedEvents(values),
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
              <SelectTrigger className="w-full">
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
  const eventScope = useWatch({ control: form.control, name: 'event_scope' })
  const editing = !!channel
  // Without a readable summary the saved SMTP values are unknown, so blank
  // fields keep them.
  const smtpUnknown = editing && !channel.smtp_summary
  const unchanged = smtpUnknown ? 'Leave blank to keep existing' : undefined

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
            <FormItem className="flex items-center justify-between gap-4 space-y-0">
              <div className="space-y-0.5">
                <FormLabel>Enabled</FormLabel>
                <FormDescription>
                  {field.value
                    ? 'This channel sends notifications.'
                    : 'Paused. Events are not sent to this channel.'}
                </FormDescription>
              </div>
              <FormControl>
                <Switch
                  checked={field.value}
                  onCheckedChange={field.onChange}
                />
              </FormControl>
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
                  <SelectTrigger className="w-full">
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
          {smtpUnknown ? (
            <p className="text-sm text-muted-foreground">
              {channel.has_smtp_config
                ? 'Oore could not read the saved SMTP settings. Leave fields blank to keep them, or fill in the ones you want to change.'
                : 'No SMTP settings are saved yet.'}
            </p>
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
                    <Input
                      type="password"
                      autoComplete="new-password"
                      placeholder={
                        editing && channel.has_smtp_config
                          ? 'Saved password'
                          : undefined
                      }
                      {...field}
                    />
                  </FormControl>
                  {editing && channel.has_smtp_config ? (
                    <FormDescription>
                      Enter a new password only to change it. Changing the host,
                      port or username needs it again.
                    </FormDescription>
                  ) : null}
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
                <Select
                  value={field.value}
                  onValueChange={field.onChange}
                  items={TLS_MODES}
                >
                  <FormControl>
                    <SelectTrigger className="w-full">
                      <SelectValue
                        placeholder={
                          smtpUnknown ? 'Leave unchanged' : 'Select TLS mode'
                        }
                      />
                    </SelectTrigger>
                  </FormControl>
                  <SelectContent>
                    {Object.entries(TLS_MODES).map(([value, label]) => (
                      <SelectItem key={value} value={value}>
                        {label}
                      </SelectItem>
                    ))}
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
                      smtpUnknown
                        ? 'Leave blank to keep existing (comma-separated)'
                        : 'alice@example.com, bob@example.com'
                    }
                    rows={3}
                    {...field}
                  />
                </FormControl>
                <FormDescription>
                  Comma-separated list of email addresses
                  {smtpUnknown
                    ? '. Leave blank to keep existing recipients.'
                    : '.'}
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
                    ? 'Replace URL'
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
                    {channel.url_host
                      ? `Sends to ${channel.url_host}. The full URL stays hidden because it can contain a token. Enter a new one to replace it.`
                      : channel.has_url
                        ? 'A URL is saved. Enter a new one to replace it.'
                        : 'No URL saved.'}
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
        name="event_scope"
        render={({ field }) => (
          <FormItem>
            <FormLabel>Events</FormLabel>
            <FormControl>
              <RadioGroup
                value={field.value}
                onValueChange={field.onChange}
                className="flex flex-wrap gap-x-6 gap-y-2"
              >
                <label className="flex items-center gap-2 text-sm">
                  <RadioGroupItem value="all" />
                  All events
                </label>
                <label className="flex items-center gap-2 text-sm">
                  <RadioGroupItem value="selected" />
                  Selected events
                </label>
              </RadioGroup>
            </FormControl>
            <FormDescription>
              {field.value === 'all'
                ? 'Sends every build, release and runner event, including events added later.'
                : 'Sends only the events you choose below.'}
            </FormDescription>
            <FormMessage />
          </FormItem>
        )}
      />

      {eventScope === 'selected' ? (
        <FormField
          control={form.control}
          name="events"
          render={({ field }) => (
            <FormItem>
              <div className="grid gap-4 sm:grid-cols-3">
                {NOTIFICATION_EVENT_GROUPS.map((group) => (
                  <fieldset key={group.label} className="space-y-2">
                    <legend className="mb-2 text-xs font-medium text-muted-foreground">
                      {group.label}
                    </legend>
                    {group.events.map((event) => (
                      <label
                        key={event.value}
                        className="flex items-center gap-2 text-sm"
                      >
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
                        {event.label}
                      </label>
                    ))}
                  </fieldset>
                ))}
              </div>
              <FormMessage />
            </FormItem>
          )}
        />
      ) : null}
    </>
  )
}
