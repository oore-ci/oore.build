import { useState } from 'react'
import {
  Plus as Add01Icon,
  ArrowUp as ArrowUp01Icon,
  Folder as Folder02Icon,
  GitBranch as GitBranchIcon,
  RefreshCw as Refresh01Icon,
} from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemMedia,
  ItemTitle,
} from '@/components/ui/item'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Spinner } from '@/components/ui/spinner'
import { useBrowseLocalGitDirectories } from '@/hooks/use-integrations'

interface LocalFolderPickerDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  enabled?: boolean
  initialPath?: string
  title?: string
  description?: string
  requireGitRepository?: boolean
  selectCurrentLabel?: string
  selectDirectoryLabel?: string
  onSelectPath: (path: string) => void
}

export default function LocalFolderPickerDialog({
  open,
  onOpenChange,
  enabled = true,
  initialPath,
  title = 'Browse Local Folders',
  description = 'Select a folder on the daemon host.',
  requireGitRepository = false,
  selectCurrentLabel = 'Use Current Folder',
  selectDirectoryLabel = 'Select',
  onSelectPath,
}: LocalFolderPickerDialogProps) {
  const [browserPath, setBrowserPath] = useState<string | undefined>(undefined)

  const {
    data: browserData,
    isLoading: browserLoading,
    isFetching: browserFetching,
    refetch: refetchBrowser,
  } = useBrowseLocalGitDirectories(browserPath, open && enabled)

  const quickJumps = (browserData?.suggestions ?? []).filter(
    (suggestion) => suggestion.path.length > 0,
  )

  const canSelectCurrent =
    !!browserData &&
    (!requireGitRepository || browserData.current_is_git_repository)
  const CurrentSelectIcon = requireGitRepository ? GitBranchIcon : Folder02Icon

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (nextOpen) {
          const candidate = initialPath?.trim()
          setBrowserPath(candidate ? candidate : undefined)
        } else {
          setBrowserPath(undefined)
        }
        onOpenChange(nextOpen)
      }}
    >
      <DialogContent className="sm:max-w-4xl">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <Item variant="muted" size="sm">
            <ItemContent>
              <ItemDescription>Current folder</ItemDescription>
              <ItemTitle className="font-mono text-xs break-all">
                {browserData?.current_path ?? 'Loading...'}
              </ItemTitle>
            </ItemContent>
          </Item>

          {quickJumps.length ? (
            <div className="flex flex-wrap items-center gap-2">
              <p className="text-xs font-medium text-muted-foreground">
                Jump to
              </p>
              {quickJumps.map((suggestion) => (
                <Button
                  key={suggestion.path}
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => setBrowserPath(suggestion.path)}
                  disabled={browserFetching}
                >
                  {suggestion.label}
                </Button>
              ))}
            </div>
          ) : null}

          <div className="flex flex-wrap items-center gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => {
                if (browserData?.parent_path) {
                  setBrowserPath(browserData.parent_path)
                }
              }}
              disabled={!browserData?.parent_path || browserFetching}
            >
              <ArrowUp01Icon />
              Up
            </Button>

            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => void refetchBrowser()}
              disabled={browserFetching}
            >
              <Refresh01Icon />
              Refresh
            </Button>

            {canSelectCurrent ? (
              <Button
                type="button"
                size="sm"
                onClick={() => {
                  onSelectPath(browserData.current_path)
                  onOpenChange(false)
                }}
              >
                <CurrentSelectIcon />
                {selectCurrentLabel}
              </Button>
            ) : null}
          </div>

          {browserLoading ? (
            <div className="flex items-center gap-2 py-3">
              <Spinner className="size-4" />
              <span className="text-sm text-muted-foreground">
                Loading folders...
              </span>
            </div>
          ) : (
            <ScrollArea className="h-80 rounded-lg ring-1 ring-foreground/10">
              {browserData?.directories.length ? (
                <div className="divide-y">
                  {browserData.directories.map((directory) => {
                    const canSelectDirectory =
                      !requireGitRepository || directory.is_git_repository
                    return (
                      <div
                        key={directory.path}
                        className="flex flex-col gap-3 px-4 py-3 md:flex-row md:items-center md:justify-between"
                      >
                        <Item
                          render={<button type="button" />}
                          className="min-w-0 flex-1 cursor-pointer text-left"
                          onClick={() => setBrowserPath(directory.path)}
                        >
                          <ItemMedia variant="icon">
                            <Folder02Icon />
                          </ItemMedia>
                          <ItemContent>
                            <ItemTitle>{directory.name}</ItemTitle>
                            <ItemDescription className="font-mono text-xs">
                              {directory.path}
                            </ItemDescription>
                          </ItemContent>
                          <ItemActions>
                            {directory.is_git_repository ? (
                              <Badge variant="secondary">Git repo</Badge>
                            ) : null}
                          </ItemActions>
                        </Item>

                        {canSelectDirectory ? (
                          <Button
                            type="button"
                            size="sm"
                            onClick={() => {
                              onSelectPath(directory.path)
                              onOpenChange(false)
                            }}
                          >
                            <Add01Icon />
                            {selectDirectoryLabel}
                          </Button>
                        ) : null}
                      </div>
                    )
                  })}
                </div>
              ) : (
                <div className="p-6">
                  <p className="text-sm text-muted-foreground">
                    No subfolders found in this location.
                  </p>
                </div>
              )}
            </ScrollArea>
          )}
        </div>

        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            onClick={() => onOpenChange(false)}
          >
            Done
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
