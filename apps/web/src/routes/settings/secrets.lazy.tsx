import { useState } from 'react'
import { createLazyFileRoute } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { Add01Icon } from '@hugeicons/core-free-icons'

import { useHasPermissions } from '@/hooks/use-permissions'
import { PageMeta } from '@/lib/seo'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { SecretsPanel } from '@/components/secrets/secrets-panel'
import { AppStoreConnectKeysPanel } from '@/components/store-credentials/app-store-connect-keys-panel'
import { GooglePlayServiceAccountPanel } from '@/components/store-credentials/google-play-service-account-panel'
import { Stack } from '@/components/layout/stack'
import { Button } from '@/components/ui/button'

export const Route = createLazyFileRoute('/settings/secrets')({
  component: SecretsPage,
})

function SecretsPage() {
  const [canWrite] = useHasPermissions(['instance_settings:write'])
  const [createOpen, setCreateOpen] = useState(false)

  return (
    <PageLayout width="wide">
      <PageMeta title="Secrets and credentials" noindex />
      <PageHeader
        title="Secrets and credentials"
        actions={
          canWrite ? (
            <Button onClick={() => setCreateOpen(true)}>
              <HugeiconsIcon icon={Add01Icon} />
              Add secret
            </Button>
          ) : undefined
        }
      />
      <Stack gap={10}>
        <SecretsPanel
          canWrite={canWrite}
          createOpen={createOpen}
          onCreateOpenChange={setCreateOpen}
          target={{ scope: 'instance' }}
          title="Instance secrets"
        />
        <Stack
          id="store-credentials"
          gap={10}
          className="scroll-mt-[calc(var(--header-height)+4rem)]"
        >
          <AppStoreConnectKeysPanel canWrite={canWrite} />
          <GooglePlayServiceAccountPanel canWrite={canWrite} />
        </Stack>
      </Stack>
    </PageLayout>
  )
}
