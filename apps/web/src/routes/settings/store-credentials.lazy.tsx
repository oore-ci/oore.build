import { createLazyFileRoute } from '@tanstack/react-router'

import { useHasPermissions } from '@/hooks/use-permissions'
import { PageMeta } from '@/lib/seo'
import PageHeader from '@/components/page-header'
import PageLayout from '@/components/page-layout'
import { AppStoreConnectKeysPanel } from '@/components/store-credentials/app-store-connect-keys-panel'
import { GooglePlayServiceAccountPanel } from '@/components/store-credentials/google-play-service-account-panel'
import { Stack } from '@/components/layout/stack'

export const Route = createLazyFileRoute('/settings/store-credentials')({
  component: StoreCredentialsPage,
})

function StoreCredentialsPage() {
  const [canWrite] = useHasPermissions(['instance_settings:write'])

  return (
    <PageLayout width="wide">
      <PageMeta title="Store credentials" noindex />
      <PageHeader title="Store credentials" />
      <Stack gap={10}>
        <AppStoreConnectKeysPanel canWrite={canWrite} />
        <GooglePlayServiceAccountPanel canWrite={canWrite} />
      </Stack>
    </PageLayout>
  )
}
