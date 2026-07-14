import { HugeiconsIcon } from '@hugeicons/react'
import { ArrowRight01Icon } from '@hugeicons/core-free-icons'
import type { PreferencesPageState } from '@/routes/settings/preferences'

export function ExternalAccessManagement({
  state,
}: {
  state: PreferencesPageState
}) {
  const {
    isOwner,
    networkSettings,
    networkSettingsQuery,
    remoteAuthMode,
    setNetworkEditorOpen,
    setOidcDialogOpen,
    setTrustedProxyDialogOpen,
  } = state
  return (
    <div className="space-y-3 border p-3">
      <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
        Manage External Access
      </p>
      <div className="grid gap-3 md:grid-cols-2">
        <button
          type="button"
          onClick={() => setNetworkEditorOpen(true)}
          disabled={!isOwner || networkSettingsQuery.isLoading}
          className="group w-full border border-border/60 bg-card p-4 text-left transition-colors hover:border-primary/40 hover:bg-primary/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 disabled:cursor-not-allowed disabled:opacity-60"
        >
          <p className="text-sm font-medium">Network settings</p>
          <p className="mt-1 text-xs text-muted-foreground">
            {networkSettings?.public_url ??
              'Set Public URL and allowed origins.'}
          </p>
          <p className="mt-3 inline-flex items-center gap-1 text-xs font-medium text-primary">
            Edit
            <HugeiconsIcon icon={ArrowRight01Icon} size={14} />
          </p>
        </button>

        <button
          type="button"
          onClick={() =>
            remoteAuthMode === 'trusted_proxy'
              ? setTrustedProxyDialogOpen(true)
              : setOidcDialogOpen(true)
          }
          disabled={!isOwner}
          className="group w-full border border-border/60 bg-card p-4 text-left transition-colors hover:border-primary/40 hover:bg-primary/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 disabled:cursor-not-allowed disabled:opacity-60"
        >
          <p className="text-sm font-medium">Identity settings</p>
          <p className="mt-1 text-xs text-muted-foreground">
            {remoteAuthMode === 'trusted_proxy'
              ? 'Update trusted proxy header, peer CIDRs, and secret.'
              : 'Update issuer and client credentials.'}
          </p>
          <p className="mt-3 inline-flex items-center gap-1 text-xs font-medium text-primary">
            Edit
            <HugeiconsIcon icon={ArrowRight01Icon} size={14} />
          </p>
        </button>
      </div>
    </div>
  )
}
