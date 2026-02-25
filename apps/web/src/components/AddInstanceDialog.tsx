import { createSignal, Show } from 'solid-js'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { useInstanceStore } from '@/stores/instance-store'

function normalizeUrl(url: string): string {
  return url.trim().replace(/\/+$/, '')
}

export default function AddInstanceDialog(props: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const [label, setLabel] = createSignal('')
  const [url, setUrl] = createSignal('http://127.0.0.1:8787')
  const [error, setError] = createSignal<string | null>(null)

  const save = () => {
    try {
      const parsed = new URL(url())
      const safeLabel = label().trim() || parsed.hostname
      const id = useInstanceStore.getState().addInstance(safeLabel, normalizeUrl(parsed.toString()))
      useInstanceStore.getState().setActiveInstance(id)
      setLabel('')
      setError(null)
      props.onOpenChange(false)
    } catch {
      setError('Enter a valid backend URL (for example http://127.0.0.1:8787).')
    }
  }

  return (
    <Show when={props.open}>
      <div class="fixed inset-0 z-50 grid place-items-center bg-black/40 p-4">
        <Card class="w-full max-w-md">
          <CardHeader>
            <CardTitle>Add backend instance</CardTitle>
          </CardHeader>
          <CardContent class="space-y-3">
            <div class="space-y-1">
              <label class="text-xs font-medium text-muted-foreground">Label</label>
              <Input
                value={label()}
                onInput={(event) => setLabel(event.currentTarget.value)}
                placeholder="Local"
              />
            </div>
            <div class="space-y-1">
              <label class="text-xs font-medium text-muted-foreground">Backend URL</label>
              <Input
                value={url()}
                onInput={(event) => setUrl(event.currentTarget.value)}
                placeholder="http://127.0.0.1:8787"
              />
            </div>
            <Show when={error()}>
              <p class="text-xs text-destructive">{error()}</p>
            </Show>
            <div class="flex justify-end gap-2">
              <Button variant="outline" onClick={() => props.onOpenChange(false)}>
                Cancel
              </Button>
              <Button onClick={save}>Save Instance</Button>
            </div>
          </CardContent>
        </Card>
      </div>
    </Show>
  )
}
