import { SettingsSurface } from '@/components/settings/settings-section'
import { ItemGroup, ItemSeparator } from '@/components/ui/item'

/** Item rows on one bordered surface, divided by separators. */
export function RowList({ children }: { children: Array<React.ReactNode> }) {
  return (
    <SettingsSurface inset={false}>
      <ItemGroup className="gap-0" role="list">
        {children.flatMap((child, index) =>
          index === 0
            ? [child]
            : [
                <ItemSeparator key={`separator-${index}`} className="my-0" />,
                child,
              ],
        )}
      </ItemGroup>
    </SettingsSurface>
  )
}
