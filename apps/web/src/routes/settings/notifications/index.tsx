import { Link, createFileRoute } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  Delete02Icon,
  InformationCircleIcon,
  TestTube01Icon,
} from '@hugeicons/core-free-icons'
import { toast } from 'sonner'

import type { NotificationChannel } from '@/lib/types'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import {
  useDeleteNotificationChannel,
  useNotificationChannels,
  useTestNotificationChannel,
} from '@/hooks/use-notification-channels'
import { PageMeta } from '@/lib/seo'
import { Alert, AlertDescription } from '@/components/ui/alert'
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
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { Skeleton } from '@/components/ui/skeleton'

export const Route = createFileRoute('/settings/notifications/')({
  staticData: { breadcrumbLabel: 'Notifications' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: NotificationsPage,
})

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

function NotificationsPage() {
  const { data, isLoading, error } = useNotificationChannels()
  const deleteMutation = useDeleteNotificationChannel()
  const testMutation = useTestNotificationChannel()

  function handleDelete(channel: NotificationChannel) {
    deleteMutation.mutate(channel.id, {
      onSuccess: () => toast.success(`Deleted channel: ${channel.name}`),
      onError: (err) => toast.error(`Failed to delete: ${err.message}`),
    })
  }

  function handleTest(channel: NotificationChannel) {
    testMutation.mutate(channel.id, {
      onSuccess: (result) => {
        if (result.success) {
          toast.success(`Test notification sent to ${channel.name}`)
        } else {
          toast.error(`Test failed: ${result.error ?? 'Unknown error'}`)
        }
      },
      onError: (err) => toast.error(`Test failed: ${err.message}`),
    })
  }

  const channels = data?.channels ?? []

  return (
    <PageLayout width="wide">
      <PageMeta title="Notifications" noindex />
      <PageHeader
        title="Notifications"
        description="Configure outbound notification channels for build status updates."
        actions={
          <Button
            render={<Link to="/settings/notifications/new" />}
            nativeButton={false}
          >
            <HugeiconsIcon icon={Add01Icon} size={16} />
            Add Channel
          </Button>
        }
      />

      {isLoading ? (
        <Card>
          <CardContent className="space-y-3">
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
          </CardContent>
        </Card>
      ) : null}

      {error ? (
        <Alert variant="destructive">
          <HugeiconsIcon icon={InformationCircleIcon} size={16} />
          <AlertDescription>
            Failed to load notification channels: {error.message}
          </AlertDescription>
        </Alert>
      ) : null}

      {!isLoading && !error ? (
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium uppercase tracking-wider text-muted-foreground">
              Channels
            </CardTitle>
          </CardHeader>
          <CardContent>
            {channels.length === 0 ? (
              <p className="py-6 text-sm text-muted-foreground">
                No notification channels configured yet.
              </p>
            ) : (
              <div className="space-y-3">
                {channels.map((channel) => (
                  <div
                    key={channel.id}
                    className="group flex items-start justify-between gap-3 border border-border/60 bg-card transition-colors hover:border-primary/30 hover:bg-primary/5"
                  >
                    <Link
                      to="/settings/notifications/$channelId"
                      params={{ channelId: channel.id }}
                      className="min-w-0 flex-1 p-4 outline-none focus-visible:ring-2 focus-visible:ring-primary/60"
                    >
                      <div className="space-y-2">
                        <p className="font-medium">{channel.name}</p>
                        <div className="flex flex-wrap items-center gap-2">
                          <Badge variant="outline">
                            {channelTypeLabel(channel.channel_type)}
                          </Badge>
                          <Badge
                            variant={channel.enabled ? 'default' : 'secondary'}
                          >
                            {channel.enabled ? 'Enabled' : 'Disabled'}
                          </Badge>
                          {channel.events.length > 0 ? (
                            <Badge variant="outline">
                              {channel.events.join(', ')}
                            </Badge>
                          ) : (
                            <Badge variant="outline">All events</Badge>
                          )}
                        </div>
                      </div>
                    </Link>

                    <div className="flex items-center gap-1 p-4 pl-0">
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => handleTest(channel)}
                        disabled={testMutation.isPending}
                      >
                        <HugeiconsIcon icon={TestTube01Icon} size={16} />
                        Test
                      </Button>
                      <AlertDialog>
                        <AlertDialogTrigger
                          render={
                            <Button variant="ghost" size="sm">
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
                              This permanently removes the channel and its
                              delivery history.
                            </AlertDialogDescription>
                          </AlertDialogHeader>
                          <AlertDialogFooter>
                            <AlertDialogCancel>Cancel</AlertDialogCancel>
                            <AlertDialogAction
                              onClick={() => handleDelete(channel)}
                              disabled={deleteMutation.isPending}
                            >
                              {deleteMutation.isPending
                                ? 'Deleting...'
                                : 'Delete'}
                            </AlertDialogAction>
                          </AlertDialogFooter>
                        </AlertDialogContent>
                      </AlertDialog>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </CardContent>
        </Card>
      ) : null}
    </PageLayout>
  )
}
