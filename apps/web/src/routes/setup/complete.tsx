import { createSignal, Show } from 'solid-js'
import { Link, createFileRoute } from '@tanstack/solid-router'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { useCompleteSetup, useSetupSummary } from '@/hooks/use-setup'
import { useSetupStore } from '@/stores/setup-store'

export const Route = createFileRoute('/setup/complete')({
  component: SetupCompletePage,
})

function SetupCompletePage() {
  const summary = useSetupSummary()
  const complete = useCompleteSetup()
  const [error, setError] = createSignal<string | null>(null)

  const finishSetup = async () => {
    setError(null)
    try {
      await complete.mutateAsync()
      useSetupStore.getState().setCurrentStep(0)
    } catch (value) {
      setError(value instanceof Error ? value.message : 'Setup completion failed')
    }
  }

  return (
    <div class="space-y-4">
      <div>
        <h2 class="text-lg font-semibold">Review and Complete</h2>
        <p class="text-sm text-muted-foreground">
          Confirm setup details and finalize the instance.
        </p>
      </div>

      <div class="space-y-2 text-sm">
        <p>
          <span class="text-muted-foreground">Instance ID:</span>{' '}
          <code>{summary.data?.instance_id ?? 'unknown'}</code>
        </p>
        <p>
          <span class="text-muted-foreground">State:</span>{' '}
          <code>{summary.data?.state ?? 'unknown'}</code>
        </p>
        <Show when={summary.data?.issuer_url}>
          <p>
            <span class="text-muted-foreground">Issuer:</span>{' '}
            <code>{summary.data?.issuer_url}</code>
          </p>
        </Show>
        <Show when={summary.data?.owner_email}>
          <p>
            <span class="text-muted-foreground">Owner:</span>{' '}
            <code>{summary.data?.owner_email}</code>
          </p>
        </Show>
      </div>

      {error() ? (
        <Alert variant="destructive">
          <AlertTitle>Completion failed</AlertTitle>
          <AlertDescription>{error()}</AlertDescription>
        </Alert>
      ) : null}

      <div class="flex gap-2">
        <Button onClick={finishSetup} disabled={complete.isPending}>
          Complete Setup
        </Button>
        <Link to="/">
          <Button variant="outline">Go to Dashboard</Button>
        </Link>
      </div>
    </div>
  )
}
