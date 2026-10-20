import { createLazyFileRoute } from '@tanstack/react-router'

import { useHasPermissions } from '@/hooks/use-permissions'
import { PageMeta } from '@/lib/seo'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { SecretsPanel } from '@/components/secrets/secrets-panel'

export const Route = createLazyFileRoute('/settings/secrets')({
  component: SecretsPage,
})

function SecretsPage() {
  const [canWrite] = useHasPermissions(['instance_settings:write'])

  return (
    <PageLayout width="wide">
      <PageMeta title="Secrets" noindex />
      <PageHeader
        title="Secrets"
        description="Encrypted values that builds read as ${{ secrets.NAME }}. Projects and pipelines can add their own."
      />
      <SecretsPanel
        canWrite={canWrite}
        target={{ scope: 'instance' }}
        title="Instance secrets"
      />
    </PageLayout>
  )
}
