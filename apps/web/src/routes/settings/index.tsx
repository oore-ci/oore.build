import { Link, createFileRoute } from '@tanstack/react-router'
import { ArrowRight as ArrowRight01Icon } from 'lucide-react'

import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { settingsGroupsForRole } from '@/components/settings/settings-navigation'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemMedia,
  ItemTitle,
} from '@/components/ui/item'
import { PageMeta } from '@/lib/seo'
import { useAuthStore } from '@/stores/auth-store'

export const Route = createFileRoute('/settings/')({
  component: SettingsHubPage,
})

function SettingsHubPage() {
  const role = useAuthStore((state) => state.user?.role)
  const groups = settingsGroupsForRole(role)

  return (
    <PageLayout>
      <PageMeta title="Settings" noindex />
      <PageHeader
        title="Settings"
        description="Configure this instance, access, delivery, and administrative history."
      />

      <div className="space-y-6">
        {groups.map((group) => (
          <section
            key={group.title}
            aria-labelledby={`settings-${group.title}`}
          >
            <h2
              id={`settings-${group.title}`}
              className="mb-2 text-sm font-semibold text-foreground"
            >
              {group.title}
            </h2>
            <ItemGroup className="gap-2">
              {group.items.map((item) => {
                const ItemIcon = item.icon

                return (
                  <Item
                    key={item.to}
                    variant="outline"
                    render={<Link to={item.to} />}
                  >
                    <ItemMedia
                      variant="icon"
                      className="size-8 text-muted-foreground group-hover/item:text-foreground"
                    >
                      <ItemIcon aria-hidden />
                    </ItemMedia>
                    <ItemContent>
                      <ItemTitle>{item.title}</ItemTitle>
                      <ItemDescription>{item.description}</ItemDescription>
                    </ItemContent>
                    <ItemActions>
                      <ArrowRight01Icon
                        className="text-muted-foreground"
                        aria-hidden
                      />
                    </ItemActions>
                  </Item>
                )
              })}
            </ItemGroup>
          </section>
        ))}
      </div>
    </PageLayout>
  )
}
