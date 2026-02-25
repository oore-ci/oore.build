import { For } from 'solid-js'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { useInstanceStore } from '@/stores/instance-store'

export default function InstanceSwitcher() {
  const instances = useInstanceStore((state) => state.instances)
  const activeInstanceId = useInstanceStore((state) => state.activeInstanceId)

  const list = () =>
    Object.values(instances()).sort((left, right) => {
      if (left.id === activeInstanceId()) return -1
      if (right.id === activeInstanceId()) return 1
      return right.addedAt - left.addedAt
    })

  return (
    <Card>
      <CardHeader>
        <CardTitle class="text-sm font-semibold">Instances</CardTitle>
      </CardHeader>
      <CardContent class="space-y-2">
        <For each={list()}>
          {(instance) => (
            <button
              type="button"
              onClick={() => useInstanceStore.getState().setActiveInstance(instance.id)}
              class={`w-full border px-3 py-2 text-left text-sm ${
                instance.id === activeInstanceId()
                  ? 'border-primary bg-primary/10'
                  : 'border-border hover:bg-accent'
              }`}
            >
              <div class="font-medium">{instance.label}</div>
              <div class="text-xs text-muted-foreground">{instance.url}</div>
            </button>
          )}
        </For>
        <Button
          size="sm"
          variant="outline"
          onClick={() => {
            if (activeInstanceId()) {
              useInstanceStore.getState().removeInstance(activeInstanceId()!)
            }
          }}
          disabled={!activeInstanceId()}
        >
          Remove active instance
        </Button>
      </CardContent>
    </Card>
  )
}
