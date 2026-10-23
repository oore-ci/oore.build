import { useState } from 'react'
import { Link } from '@tanstack/react-router'
import { HugeiconsIcon } from '@hugeicons/react'
import { PlayStoreIcon } from '@hugeicons/core-free-icons'

import { relativeTime } from '@/lib/format-utils'
import { useSecrets } from '@/hooks/use-secrets'
import { CollectionError } from '@/components/collection'
import { SecretValueDialog } from '@/components/secrets/secret-value-dialog'
import {
  SettingsSection,
  SettingsSurface,
} from '@/components/settings/settings-section'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemMedia,
  ItemTitle,
} from '@/components/ui/item'
import { Skeleton } from '@/components/ui/skeleton'

/** The secret name new Google Play publish settings pick by default. */
export const PLAY_SERVICE_ACCOUNT_SECRET = 'PLAY_SERVICE_ACCOUNT'

/**
 * Google Play keys are instance secrets, so a project or pipeline secret with
 * the same name still overrides the shared one.
 */
export function GooglePlayServiceAccountPanel({
  canWrite,
}: {
  canWrite: boolean
}) {
  const secretsQuery = useSecrets({ scope: 'instance' })
  const [dialogOpen, setDialogOpen] = useState(false)
  const stored = secretsQuery.data?.find(
    (secret) => secret.name === PLAY_SERVICE_ACCOUNT_SECRET,
  )

  return (
    <SettingsSection title="Google Play service account">
      {secretsQuery.error ? (
        <CollectionError
          title="Failed to load instance secrets"
          description={secretsQuery.error.message}
          onRetry={() => void secretsQuery.refetch()}
        />
      ) : secretsQuery.isLoading ? (
        <Skeleton className="h-16 w-full" />
      ) : (
        <SettingsSurface inset={false}>
          <Item size="sm" className="flex-wrap">
            <ItemMedia variant="icon">
              <HugeiconsIcon icon={PlayStoreIcon} />
            </ItemMedia>
            <ItemContent className="min-w-0">
              <ItemTitle className="font-mono">
                {PLAY_SERVICE_ACCOUNT_SECRET}
              </ItemTitle>
              <ItemDescription>
                {stored ? (
                  <>
                    Instance secret · Updated {relativeTime(stored.updated_at)}
                  </>
                ) : (
                  'Not stored yet. Add the JSON key once and new pipelines use it on the Publishing tab.'
                )}
              </ItemDescription>
            </ItemContent>
            <ItemActions className="max-sm:w-full max-sm:justify-end">
              {stored ? <Badge variant="secondary">Shared</Badge> : null}
              {canWrite ? (
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => setDialogOpen(true)}
                >
                  {stored ? 'Replace key' : 'Add service account'}
                </Button>
              ) : null}
            </ItemActions>
          </Item>
        </SettingsSurface>
      )}
      <p className="text-xs text-muted-foreground">
        Stored with the other{' '}
        <Link to="/settings/secrets" className="underline underline-offset-4">
          instance secrets
        </Link>
        . A project or pipeline secret with the same name overrides it.{' '}
        <a
          href="https://docs.oore.build/build/distribute/google-play#1-store-the-service-account-key-as-a-secret"
          target="_blank"
          rel="noopener noreferrer"
          className="underline underline-offset-4"
        >
          Learn more
        </a>
      </p>
      <SecretValueDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        name={stored ? PLAY_SERVICE_ACCOUNT_SECRET : undefined}
        defaultName={PLAY_SERVICE_ACCOUNT_SECRET}
        target={{ scope: 'instance' }}
      />
    </SettingsSection>
  )
}
