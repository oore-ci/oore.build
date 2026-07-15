import { useMemo, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  Add01Icon,
  ArrowUp01Icon,
  Folder02Icon,
  GitBranchIcon,
  Refresh01Icon,
} from '@hugeicons/core-free-icons'

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

  const quickJumps = useMemo(
    () => (browserData?.suggestions ?? []).filter((s) => s.path.length > 0),
    [browserData?.suggestions],
  )

  const canSelectCurrent =
    !!browserData &&
    (!requireGitRepository || browserData.current_is_git_repository)
  const currentSelectIcon = requireGitRepository ? GitBranchIcon : Folder02Icon

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
          <div className="border bg-muted/20 p-3">
            <p className="text-xs text-muted-foreground">Current folder</p>
            <p className="mt-1 break-all font-mono text-xs">
              {browserData?.current_path ?? 'Loading...'}
            </p>
          </div>

          {quickJumps.length ? (
            <div className="flex flex-wrap items-center gap-2">
              <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
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
              <HugeiconsIcon icon={ArrowUp01Icon} />
              Up
            </Button>

            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => void refetchBrowser()}
              disabled={browserFetching}
            >
              <HugeiconsIcon icon={Refresh01Icon} />
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
                <HugeiconsIcon icon={currentSelectIcon} />
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
            <ScrollArea className="h-80 border">
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
                        <button
                          type="button"
                          className="group min-w-0 flex-1 border border-transparent px-3 py-2 text-left transition-colors hover:border-primary/30 hover:bg-primary/5 focus-visible:ring-2 focus-visible:ring-primary/60 focus-visible:outline-none"
                          onClick={() => setBrowserPath(directory.path)}
                        >
                          <div className="flex items-center gap-2">
                            <HugeiconsIcon icon={Folder02Icon} size={14} />
                            <p className="truncate text-sm font-medium">
                              {directory.name}
                            </p>
                            {directory.is_git_repository ? (
                              <Badge variant="success">Git repo</Badge>
                            ) : null}
                          </div>
                          <p className="mt-1 truncate font-mono text-xs text-muted-foreground">
                            {directory.path}
                          </p>
                        </button>

                        {canSelectDirectory ? (
                          <Button
                            type="button"
                            size="sm"
                            onClick={() => {
                              onSelectPath(directory.path)
                              onOpenChange(false)
                            }}
                          >
                            <HugeiconsIcon icon={Add01Icon} />
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
