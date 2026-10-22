import { createLazyFileRoute, Link, useNavigate } from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { HugeiconsIcon } from '@hugeicons/react'
import { Delete02Icon, TestTube01Icon } from '@hugeicons/core-free-icons'
import { toast } from '@/lib/toast'
import {
  useDeleteNotificationChannel,
  useNotificationChannel,
  useNotificationDeliveries,
  useTestNotificationChannel,
  useUpdateNotificationChannel,
} from '@/hooks/use-notification-channels'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import { fromClientChannel } from '@/lib/notification-channels'
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
import { Alert, AlertDescription } from '@/components/ui/alert'
import { DangerZone } from '@/components/danger-zone'
import { FormActions } from '@/components/form-actions'
import { RelativeTime } from '@/components/relative-time'
import { StatusBadge } from '@/components/status-badge'
import { Button } from '@/components/ui/button'
import {
  SettingsSection,
  SettingsSurface,
} from '@/components/settings/settings-section'
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from '@/components/ui/empty'
import { Form } from '@/components/ui/form'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemSeparator,
  ItemTitle,
} from '@/components/ui/item'
import {
  channelTypeLabel,
  editNotificationChannelSchema,
  NotificationChannelFormFields,
  notificationChannelDefaults,
  notificationChannelEditValues,
  notificationEventLabel,
  type NotificationChannelFormValues,
  updateNotificationChannelRequest,
} from './-notification-form'

export const Route = createLazyFileRoute('/settings/notifications/$channelId')({
  component: NotificationChannelDetailPage,
})

function NotificationChannelDetailPage() {
  const { channelId } = Route.useParams()
  const navigate = useNavigate()
  const {
    data: channelData,
    error: channelError,
    isLoading,
    refetch: refetchChannel,
  } = useNotificationChannel(channelId)
  const {
    data: deliveriesData,
    error: deliveriesError,
    isLoading: deliveriesLoading,
    refetch: refetchDeliveries,
  } = useNotificationDeliveries(channelId)
  const updateMutation = useUpdateNotificationChannel()
  const deleteMutation = useDeleteNotificationChannel()
  const testMutation = useTestNotificationChannel()
  const channel = channelData?.channel
    ? fromClientChannel(channelData.channel)
    : undefined
  const deliveries = deliveriesData?.deliveries ?? []
  const form = useForm<NotificationChannelFormValues>({
    resolver: zodResolver(
      editNotificationChannelSchema(Boolean(channel?.smtp_summary)),
    ),
    defaultValues: notificationChannelDefaults,
    values: channel ? notificationChannelEditValues(channel) : undefined,
    mode: 'onBlur',
  })

  function onSubmit(values: NotificationChannelFormValues) {
    updateMutation.mutate(
      { id: channelId, data: updateNotificationChannelRequest(values) },
      {
        onSuccess: () => {
          toast.success('Channel updated')
          form.reset(values)
        },
        onError: (error) => toast.error(getApiErrorMessage(error, {})),
      },
    )
  }

  function handleDelete() {
    deleteMutation.mutate(channelId, {
      onSuccess: () => {
        toast.success('Channel deleted')
        void navigate({ to: '/settings/notifications' })
      },
      onError: (error) => toast.error(`Failed to delete: ${error.message}`),
    })
  }

  function handleTest() {
    testMutation.mutate(channelId, {
      onSuccess: (result) => {
        if (result.success) toast.success('Test notification sent')
        else toast.error(`Test failed: ${result.error ?? 'Unknown error'}`)
      },
      onError: (error) => toast.error(`Test failed: ${error.message}`),
    })
  }

  if (isLoading) {
    return (
      <PageLayout width="wide">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-64 w-full" />
      </PageLayout>
    )
  }

  if (channelError) {
    return (
      <PageLayout width="wide">
        <PageHeader title="Notification channel" />
        <Alert variant="destructive">
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <span>
              Failed to load notification channel: {channelError.message}
            </span>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => void refetchChannel()}
            >
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      </PageLayout>
    )
  }

  if (!channel) {
    return (
      <PageLayout width="wide">
        <PageHeader title="Channel not found" />
        <Empty>
          <EmptyHeader>
            <EmptyTitle>Channel not found</EmptyTitle>
            <EmptyDescription>
              This notification channel does not exist or has been deleted.
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      </PageLayout>
    )
  }

  return (
    <PageLayout width="wide">
      <PageMeta title={`${channel.name} notifications`} noindex />
      <PageHeader
        title={channel.name}
        description={`${channelTypeLabel(channel.channel_type)} notification channel`}
        actions={
          <Button
            variant="outline"
            onClick={handleTest}
            disabled={testMutation.isPending}
          >
            <HugeiconsIcon icon={TestTube01Icon} />
            {testMutation.isPending ? 'Sending...' : 'Test'}
          </Button>
        }
      />

      <Form {...form}>
        <form
          onSubmit={form.handleSubmit(onSubmit)}
          className="flex flex-col gap-4"
        >
          <SettingsSection
            title="Channel"
            description="Where updates go, and which events send them."
          >
            <SettingsSurface className="flex flex-col gap-4">
              <NotificationChannelFormFields channel={channel} form={form} />
            </SettingsSurface>
          </SettingsSection>
          <FormActions
            disabled={!form.formState.isDirty}
            disabledReason="No changes to save"
            isPending={updateMutation.isPending}
            submitLabel="Save changes"
          />
        </form>
      </Form>

      <SettingsSection
        title="Delivery history"
        description="The most recent attempts to reach this channel."
      >
        <SettingsSurface inset={false}>
          {deliveriesLoading ? (
            <div
              className="space-y-2 p-4 sm:p-5"
              aria-label="Loading delivery history"
            >
              <Skeleton className="h-12 w-full" />
              <Skeleton className="h-12 w-full" />
            </div>
          ) : deliveriesError ? (
            <div className="p-4 sm:p-5">
              <Alert variant="destructive">
                <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                  <span>
                    Failed to load delivery history: {deliveriesError.message}
                  </span>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() => void refetchDeliveries()}
                  >
                    Retry
                  </Button>
                </AlertDescription>
              </Alert>
            </div>
          ) : deliveries.length === 0 ? (
            <Empty className="py-8">
              <EmptyHeader>
                <EmptyTitle>No deliveries yet</EmptyTitle>
                <EmptyDescription>
                  Delivery attempts will appear here after this channel is
                  triggered. Use Test to send one now.
                </EmptyDescription>
              </EmptyHeader>
            </Empty>
          ) : (
            <ItemGroup>
              {deliveries.map((delivery, index) => (
                <div key={delivery.id}>
                  {index > 0 ? <ItemSeparator /> : null}
                  <Item size="sm">
                    <ItemContent className="min-w-0">
                      <ItemTitle>
                        {notificationEventLabel(delivery.event_type)}
                      </ItemTitle>
                      <ItemDescription>
                        <RelativeTime at={delivery.created_at} />
                        {delivery.attempt_count > 1
                          ? ` · ${delivery.attempt_count} attempts`
                          : ''}
                        {delivery.build_id ? (
                          <>
                            {' · '}
                            <Link
                              to="/builds/$buildId"
                              params={{ buildId: delivery.build_id }}
                              className="underline underline-offset-4"
                            >
                              View build
                            </Link>
                          </>
                        ) : null}
                      </ItemDescription>
                      {delivery.last_error ? (
                        <ItemDescription className="font-mono text-xs break-words">
                          {delivery.last_error}
                        </ItemDescription>
                      ) : null}
                    </ItemContent>
                    <ItemActions>
                      <StatusBadge kind="delivery" value={delivery.status} />
                    </ItemActions>
                  </Item>
                </div>
              ))}
            </ItemGroup>
          )}
        </SettingsSurface>
      </SettingsSection>

      <DangerZone
        title="Delete this channel"
        description={`Permanently removes "${channel.name}" and its delivery history. Pipelines that notified it stop sending.`}
        action={
          <AlertDialog>
            <AlertDialogTrigger
              render={
                <Button variant="destructive">
                  <HugeiconsIcon icon={Delete02Icon} />
                  Delete channel
                </Button>
              }
            />
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>
                  Delete notification channel?
                </AlertDialogTitle>
                <AlertDialogDescription>
                  This permanently removes the channel and its delivery history.
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
        }
      />
    </PageLayout>
  )
}
