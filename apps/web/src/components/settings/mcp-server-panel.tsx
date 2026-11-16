import {
  useInstancePreferences,
  useUpdateInstancePreferences,
} from '@/hooks/use-artifact-storage'
import { useAuthStore } from '@/stores/auth-store'
import { toast } from '@/lib/toast'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemTitle,
} from '@/components/ui/item'
import { Skeleton } from '@/components/ui/skeleton'
import { Switch } from '@/components/ui/switch'
import {
  SettingsSection,
  SettingsSurface,
} from '@/components/settings/settings-section'

/** The owner's switch for the MCP server that AI agents connect to. */
export function McpServerPanel() {
  const preferencesQuery = useInstancePreferences()
  const mutation = useUpdateInstancePreferences()
  const user = useAuthStore((s) => s.user)
  const isOwner = user?.role === 'owner'
  const preferences = preferencesQuery.data

  if (preferencesQuery.error) return null

  function update(checked: boolean) {
    if (!preferences || !isOwner || mutation.isPending) return
    mutation.mutate(
      {
        key_storage_mode: preferences.key_storage_mode,
        mcp_enabled: checked,
      },
      {
        onSuccess: () =>
          toast.success(
            checked ? 'MCP server turned on.' : 'MCP server turned off.',
          ),
        onError: (error) =>
          toast.error(`Failed to update the MCP server: ${error.message}`),
      },
    )
  }

  return (
    <SettingsSection
      title="AI agents"
      hint="Agents connect to /mcp with an API token and can do only what that token allows."
    >
      {preferencesQuery.isLoading || !preferences ? (
        <Skeleton className="h-16 w-full" />
      ) : (
        <SettingsSurface inset={false}>
          <Item>
            <ItemContent>
              <ItemTitle>MCP server</ItemTitle>
              <ItemDescription>
                {preferences.mcp_enabled
                  ? 'On. Agents can connect with an API token.'
                  : 'Off. Agents get not found at /mcp.'}
              </ItemDescription>
              {!isOwner ? (
                <ItemDescription>Only the owner can change this.</ItemDescription>
              ) : null}
            </ItemContent>
            <ItemActions className="ml-auto">
              <Switch
                id="mcp-server"
                aria-label="MCP server"
                checked={preferences.mcp_enabled}
                disabled={!isOwner || mutation.isPending}
                onCheckedChange={update}
              />
            </ItemActions>
          </Item>
        </SettingsSurface>
      )}
    </SettingsSection>
  )
}
