import { Link, createFileRoute } from '@tanstack/react-router'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import {
  ArrowRight as ArrowRight01Icon,
} from 'lucide-react'

import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { settingsGroupsForRole } from '@/components/settings/settings-navigation'
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
            <div className="divide-y border bg-card">
              {group.items.map((item) => (
                <Link
                  key={item.to}
                  to={item.to}
                  className="group grid min-h-16 grid-cols-[2rem_minmax(0,1fr)_auto] items-center gap-3 px-3 py-3 transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-ring focus-visible:ring-inset sm:px-4"
                >
                  <span className="flex size-8 items-center justify-center border bg-background text-muted-foreground group-hover:text-foreground">
                    <DynamicLucideIcon icon={item.icon} aria-hidden />
                  </span>
                  <span className="min-w-0">
                    <span className="block text-sm font-medium">
                      {item.title}
                    </span>
                    <span className="block text-xs leading-relaxed text-muted-foreground">
                      {item.description}
                    </span>
                  </span>
                  <DynamicLucideIcon
                    icon={ArrowRight01Icon}
                    className="text-muted-foreground"
                    aria-hidden
                  />
                </Link>
              ))}
            </div>
          </section>
        ))}
      </div>
    </PageLayout>
  )
}
