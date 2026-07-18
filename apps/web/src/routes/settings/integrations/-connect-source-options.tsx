import { Link } from '@tanstack/react-router'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import {
  Link2 as Link04Icon,
} from 'lucide-react'

import { buttonVariants } from '@/components/ui/button-variants'

const providers = [
  {
    name: 'GitHub',
    to: '/settings/integrations/github' as const,
    description:
      'Create and install a GitHub App for repository discovery and webhook events.',
    heading: 'Requested access',
    items: [
      'Repository contents and metadata read access.',
      'Pull request read plus statuses/checks write access.',
      'Push and pull request webhook events.',
    ],
  },
  {
    name: 'GitLab',
    to: '/settings/integrations/gitlab' as const,
    description:
      'Connect GitLab.com or a self-managed host with a personal access token or OAuth application.',
    heading: 'Token scopes',
    items: [
      'Use read_user, read_api, and read_repository.',
      'Avoid full api unless a write feature needs it.',
    ],
  },
]

export function ConnectSourceOptions() {
  return (
    <section
      className="flex flex-col gap-4"
      aria-labelledby="connect-source-title"
    >
      <div>
        <h2 id="connect-source-title" className="text-sm font-semibold">
          Connect a source
        </h2>
        <p className="mt-1 text-sm text-muted-foreground">
          Choose a provider to start discovering repositories.
        </p>
      </div>
      <div className="grid items-stretch gap-4 md:grid-cols-2">
        {providers.map((provider) => (
          <article key={provider.name} className="flex flex-col border bg-card">
            <div className="border-b px-4 py-3">
              <h3 className="text-sm font-semibold">{provider.name}</h3>
              <p className="mt-1 text-sm text-muted-foreground">
                {provider.description}
              </p>
            </div>
            <div className="flex-1 p-4">
              <div className="flex h-full flex-col gap-2 text-xs text-muted-foreground">
                <p className="font-medium text-foreground">
                  {provider.heading}
                </p>
                <ul className="flex list-disc flex-col gap-1 pl-4 leading-relaxed">
                  {provider.items.map((item) => (
                    <li key={item}>{item}</li>
                  ))}
                </ul>
              </div>
            </div>
            <div className="mt-auto border-t px-4 py-3">
              <Link
                to={provider.to}
                className={buttonVariants({
                  size: 'sm',
                  className: 'w-full sm:w-auto',
                })}
              >
                <DynamicLucideIcon
                  icon={Link04Icon}
                  data-icon="inline-start"
                  aria-hidden
                />
                Connect {provider.name}
              </Link>
            </div>
          </article>
        ))}
      </div>
    </section>
  )
}
