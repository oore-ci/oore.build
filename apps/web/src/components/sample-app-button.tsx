import { useNavigate } from '@tanstack/react-router'
import { toast } from '@/lib/toast'
import { useCreateSampleProject } from '@/hooks/use-projects'
import { useSetupStatus } from '@/hooks/use-setup'
import { Button } from '@/components/ui/button'
import { Spinner } from '@/components/ui/spinner'

/**
 * Local Only: add a Flutter sample app the backend creates, to see a first
 * build before connecting your own code. Renders nothing on other modes.
 */
export function SampleAppButton({
  onCreated,
  variant = 'outline',
}: {
  onCreated?: () => void
  variant?: 'outline' | 'ghost' | 'link'
}) {
  const setupStatus = useSetupStatus()
  const mutation = useCreateSampleProject()
  const navigate = useNavigate()
  if (setupStatus.data?.runtime_mode !== 'local') return null

  return (
    <Button
      type="button"
      variant={variant}
      size="sm"
      disabled={mutation.isPending}
      onClick={() =>
        mutation.mutate(undefined, {
          onSuccess: (response) => {
            toast.success('Sample app added')
            onCreated?.()
            void navigate({
              to: '/projects/$projectId',
              params: { projectId: response.project.id },
            })
          },
          onError: (error) => toast.error(error.message),
        })
      }
    >
      {mutation.isPending ? <Spinner data-icon="inline-start" /> : null}
      {mutation.isPending
        ? 'Creating the sample...'
        : 'Try a sample Flutter app'}
    </Button>
  )
}
