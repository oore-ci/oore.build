import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import type { BuildScope } from '@/stores/build-scope-store'

/** "Mine" is builds you started plus builds of apps you pinned. */
export function BuildScopeToggle({
  scope,
  onChange,
}: {
  scope: BuildScope
  onChange: (scope: BuildScope) => void
}) {
  return (
    <ToggleGroup
      aria-label="Whose builds"
      value={[scope]}
      variant="outline"
      size="sm"
      onValueChange={(value) => {
        const next = value.at(0)
        if (next === 'mine' || next === 'all') onChange(next)
      }}
    >
      <ToggleGroupItem
        value="mine"
        title="Builds you started and builds of apps you pinned"
      >
        Mine
      </ToggleGroupItem>
      <ToggleGroupItem value="all">Everyone</ToggleGroupItem>
    </ToggleGroup>
  )
}
