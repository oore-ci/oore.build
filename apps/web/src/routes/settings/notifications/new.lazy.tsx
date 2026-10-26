import { createLazyFileRoute, useNavigate } from '@tanstack/react-router'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { toast } from '@/lib/toast'
import { useCreateNotificationChannel } from '@/hooks/use-notification-channels'
import { PageMeta } from '@/lib/seo'
import PageLayout from '@/components/page-layout'
import PageHeader from '@/components/page-header'
import { FormActions } from '@/components/form-actions'
import {
  SettingsSection,
  SettingsSurface,
} from '@/components/settings/settings-section'
import { Form } from '@/components/ui/form'
import { getApiErrorMessage } from '@/lib/api-client/api-error'
import {
  createNotificationChannelRequest,
  createNotificationChannelSchema,
  NotificationChannelFormFields,
  notificationChannelDefaults,
  type NotificationChannelFormValues,
} from './-notification-form'

export const Route = createLazyFileRoute('/settings/notifications/new')({
  component: NewNotificationChannelPage,
})

function NewNotificationChannelPage() {
  const navigate = useNavigate()
  const createMutation = useCreateNotificationChannel()
  const form = useForm<NotificationChannelFormValues>({
    resolver: zodResolver(createNotificationChannelSchema),
    defaultValues: notificationChannelDefaults,
    mode: 'onBlur',
  })

  function onSubmit(values: NotificationChannelFormValues) {
    createMutation.mutate(createNotificationChannelRequest(values), {
      onSuccess: () => {
        toast.success('Notification channel created')
        void navigate({ to: '/settings/notifications' })
      },
      onError: (error) => toast.error(getApiErrorMessage(error, {})),
    })
  }

  return (
    <PageLayout>
      <PageMeta title="New notification channel" noindex />
      <PageHeader title="New notification channel" />

      <Form {...form}>
        <form
          onSubmit={form.handleSubmit(onSubmit)}
          className="flex flex-col gap-8"
        >
          <SettingsSection title="Channel">
            <SettingsSurface className="flex flex-col gap-4">
              <NotificationChannelFormFields form={form} />
            </SettingsSurface>
          </SettingsSection>
          <FormActions
            isPending={createMutation.isPending}
            onCancel={() => void navigate({ to: '/settings/notifications' })}
            pendingLabel="Creating..."
            submitLabel="Create channel"
          />
        </form>
      </Form>
    </PageLayout>
  )
}
