import { useState } from 'react'
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
import { Row } from '@/components/layout/row'
import { Stack } from '@/components/layout/stack'
import { Text } from '@/components/layout/text'
import { textVariants } from '@/lib/recipes'

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
            <Row gap={2} className="flex-wrap">
              <Text variant="caption" className="font-medium">
                Jump to
              </Text>
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
            </Row>
          ) : null}

          <Row gap={2} className="flex-wrap">
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
              <HugeiconsIcon icon={ArrowUp01Icon} data-icon="inline-start" />
              Up
            </Button>

            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => void refetchBrowser()}
              disabled={browserFetching}
            >
              <HugeiconsIcon icon={Refresh01Icon} data-icon="inline-start" />
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
                <HugeiconsIcon
                  icon={CurrentSelectIcon}
                  data-icon="inline-start"
                />
                {selectCurrentLabel}
              </Button>
            ) : null}
          </Row>

          {browserLoading ? (
            <Row gap={2} className="py-3">
              <Spinner className="size-4" />
              <span className={textVariants({ variant: 'muted' })}>
                Loading folders...
              </span>
            </Row>
          ) : (
            <ScrollArea className="h-80 rounded-lg ring-1 ring-foreground/10">
              {browserData?.directories.length ? (
                <div className="divide-y">
                  {browserData.directories.map((directory) => {
                    const canSelectDirectory =
                      !requireGitRepository || directory.is_git_repository
                    return (
                      <Stack
                        key={directory.path}
                        gap={3}
                        className="px-4 py-3 md:flex-row md:items-center md:justify-between"
                      >
                        <Item
                          render={<button type="button" />}
                          className="min-w-0 flex-1 cursor-pointer text-left"
                          onClick={() => setBrowserPath(directory.path)}
                        >
                          <ItemMedia variant="icon">
                            <HugeiconsIcon icon={Folder02Icon} />
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
                            <HugeiconsIcon icon={Add01Icon} />
                            {selectDirectoryLabel}
                          </Button>
                        ) : null}
                      </Stack>
                    )
                  })}
                </div>
              ) : (
                <div className="p-6">
                  <Text variant="muted">
                    No subfolders found in this location.
                  </Text>
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
