import { useId } from 'react'
import { createFileRoute, Link } from '@tanstack/react-router'
import type {
  NotificationPreferences,
  PersonalBuildEvents,
  WeeklyDigest,
} from '@oore/client/models'

import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import {
  SettingsSection,
  SettingsSurface,
  SettingsSurfaceSeparator,
} from '@/components/settings/settings-section'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'
import { Switch } from '@/components/ui/switch'
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import { useNotificationPermission } from '@/hooks/use-build-notification'
import {
  useNotificationPreferences,
  useSendWeeklyDigest,
  useUpdateNotificationPreferences,
  useWeeklyDigest,
} from '@/hooks/use-personal'
import { useHasPermission } from '@/hooks/use-permissions'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { PageMeta } from '@/lib/seo'
import { toast } from '@/lib/toast'

export const Route = createFileRoute('/account/notifications')({
  staticData: {
    breadcrumb: {
      title: 'Your notifications',
    },
  },
  beforeLoad: (context) => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id, context)
  },
  component: NotificationSettingsPage,
})

const BUILD_EVENT_LABELS = {
  off: 'Never',
  failed: 'When they fail',
  all: 'When they finish',
} as const satisfies Record<PersonalBuildEvents, string>

function ToggleRow({
  checked,
  description,
  disabled,
  label,
  onChange,
}: {
  checked: boolean
  description: React.ReactNode
  disabled?: boolean
  label: string
  onChange: (checked: boolean) => void
}) {
  const id = useId()
  return (
    <Row gap={4} className="items-start justify-between">
      <Stack gap={0.5} className="min-w-0">
        <Label htmlFor={id}>{label}</Label>
        <Text variant="muted">{description}</Text>
      </Stack>
      <Switch
        id={id}
        checked={checked}
        disabled={disabled}
        onCheckedChange={onChange}
      />
    </Row>
  )
}

function minutes(seconds: number) {
  return `${Math.max(1, Math.round(seconds / 60))} min`
}

function DigestPreview({ digest }: { digest: WeeklyDigest }) {
  if (digest.apps.length === 0 && digest.expiring.length === 0) {
    return (
      <Text variant="muted">A quiet week: nothing finished or shipped.</Text>
    )
  }
  return (
    <Stack gap={3}>
      {digest.apps.map((app) => (
        <Stack gap={0.5} key={app.project_id}>
          <span className="text-sm font-medium">{app.name}</span>
          <Text variant="muted">
            {app.builds} builds, {app.succeeded} succeeded, {app.failed} failed
            {app.slowest_pipeline
              ? `. Slowest: ${app.slowest_pipeline.name}, about ${minutes(app.slowest_pipeline.median_seconds)}`
              : ''}
            {app.releases.length
              ? `. Releases: ${app.releases.map((release) => release.version).join(', ')}`
              : ''}
          </Text>
        </Stack>
      ))}
      {digest.expiring.length ? (
        <Stack gap={0.5}>
          <span className="text-sm font-medium">Expiring soon</span>
          {digest.expiring.map((expiry) => (
            <Text variant="muted" key={`${expiry.project_name}-${expiry.name}`}>
              {expiry.name} ({expiry.project_name}),{' '}
              {new Date(expiry.expires_at * 1000).toLocaleDateString()}
            </Text>
          ))}
        </Stack>
      ) : null}
    </Stack>
  )
}

function NotificationSettingsPage() {
  const preferencesQuery = useNotificationPreferences()
  const updateMutation = useUpdateNotificationPreferences()
  const digestQuery = useWeeklyDigest()
  const sendDigest = useSendWeeklyDigest()
  const browser = useNotificationPermission()
  const canAddChannel = useHasPermission('instance_settings:write')
  const data = preferencesQuery.data

  function save(change: Partial<NotificationPreferences>) {
    if (!data) return
    updateMutation.mutate(
      { ...data.preferences, ...change },
      {
        onError: (error) =>
          toast.error(getApiErrorMessage(error, {}) || 'Could not save'),
      },
    )
  }

  return (
    <PageLayout>
      <PageMeta title="Your notifications" noindex />
      <PageHeader
        title="Your notifications"
        description="What reaches you, and how. Channels under Settings > Notifications still post for the whole team."
      />
      {!data ? (
        <Skeleton className="h-96 w-full" />
      ) : (
        <Stack gap={8}>
          <SettingsSection title="What reaches you">
            <SettingsSurface>
              <Stack gap={4}>
                <Row gap={4} className="items-start justify-between">
                  <Stack gap={0.5} className="min-w-0">
                    <Label>Builds you start</Label>
                    <Text variant="muted">
                      Builds you run from Oore, the CLI or the API.
                    </Text>
                  </Stack>
                  <Select
                    value={data.preferences.my_builds}
                    items={BUILD_EVENT_LABELS}
                    onValueChange={(value) => {
                      if (value) save({ my_builds: value })
                    }}
                  >
                    <SelectTrigger
                      className="w-44"
                      aria-label="Builds you start"
                    >
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {Object.entries(BUILD_EVENT_LABELS).map(
                        ([value, label]) => (
                          <SelectItem key={value} value={value}>
                            {label}
                          </SelectItem>
                        ),
                      )}
                    </SelectContent>
                  </Select>
                </Row>
                <SettingsSurfaceSeparator />
                <ToggleRow
                  label="Approvals you can give"
                  description="A gated build waits in an app where you are a maintainer."
                  checked={data.preferences.approvals}
                  onChange={(approvals) => save({ approvals })}
                />
                <SettingsSurfaceSeparator />
                <ToggleRow
                  label="Releases of apps you pin"
                  description={
                    <>
                      Review, rollout and store changes. Pin an app with the
                      star on the{' '}
                      <Link
                        to="/projects"
                        className="underline underline-offset-4"
                      >
                        Apps
                      </Link>{' '}
                      page.
                    </>
                  }
                  checked={data.preferences.followed_releases}
                  onChange={(followed_releases) => save({ followed_releases })}
                />
              </Stack>
            </SettingsSurface>
          </SettingsSection>

          <SettingsSection title="How it reaches you">
            <SettingsSurface>
              <Stack gap={4}>
                <ToggleRow
                  label="Browser notifications"
                  description={
                    browser.permission === 'denied'
                      ? 'Your browser blocks notifications from Oore. Allow them in the site settings.'
                      : 'While Oore is open in a tab you are not looking at. The bell lists them either way.'
                  }
                  checked={data.preferences.browser}
                  disabled={browser.permission === 'unsupported'}
                  onChange={(value) => {
                    if (value && browser.permission === 'default')
                      browser.request()
                    save({ browser: value })
                  }}
                />
                <SettingsSurfaceSeparator />
                <ToggleRow
                  label={`Email to ${data.email_address}`}
                  description={
                    data.email_available
                      ? 'Sent through the instance email channel.'
                      : 'Needs an email channel under Settings > Notifications. Oore sends personal email through its server.'
                  }
                  checked={data.preferences.email}
                  disabled={!data.email_available && !data.preferences.email}
                  onChange={(email) => save({ email })}
                />
              </Stack>
            </SettingsSurface>
            {!data.email_available && canAddChannel ? (
              <Alert>
                <AlertDescription>
                  <Link
                    to="/settings/notifications/new"
                    className="underline underline-offset-4"
                  >
                    Add an email channel
                  </Link>{' '}
                  so people can get personal email and the weekly digest.
                </AlertDescription>
              </Alert>
            ) : null}
          </SettingsSection>

          <SettingsSection title="Weekly digest">
            <SettingsSurface>
              <Stack gap={4}>
                <ToggleRow
                  label="Email me a weekly digest"
                  description="Monday morning (UTC): what shipped, success rate, the slowest pipeline and anything expiring, for the apps you pin, or all your apps if you pin none."
                  checked={data.preferences.weekly_digest}
                  disabled={
                    !data.email_available && !data.preferences.weekly_digest
                  }
                  onChange={(weekly_digest) => save({ weekly_digest })}
                />
                <SettingsSurfaceSeparator />
                <Stack gap={2}>
                  <Row gap={2} className="justify-between">
                    <span className="text-sm font-medium">
                      This week so far
                    </span>
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={!data.email_available || sendDigest.isPending}
                      onClick={() =>
                        sendDigest.mutate(undefined, {
                          onSuccess: () =>
                            toast.success(
                              `Digest sent to ${data.email_address}`,
                            ),
                          onError: (error) =>
                            toast.error(getApiErrorMessage(error, {})),
                        })
                      }
                    >
                      {sendDigest.isPending ? (
                        <Spinner data-icon="inline-start" />
                      ) : null}
                      Email it to me now
                    </Button>
                  </Row>
                  {digestQuery.data ? (
                    <DigestPreview digest={digestQuery.data} />
                  ) : (
                    <Skeleton className="h-16 w-full" />
                  )}
                </Stack>
              </Stack>
            </SettingsSurface>
          </SettingsSection>
        </Stack>
      )}
    </PageLayout>
  )
}
