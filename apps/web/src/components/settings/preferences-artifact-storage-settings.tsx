import { HugeiconsIcon } from '@hugeicons/react'
import { Folder02Icon } from '@hugeicons/core-free-icons'
import type { ArtifactStoragePageState } from '@/components/settings/use-artifact-storage-page-state'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Spinner } from '@/components/ui/spinner'
import { ArtifactObjectStorageFields } from '@/components/settings/preferences-artifact-object-storage-fields'

export function ArtifactStorageSettings({
  state,
}: {
  state: ArtifactStoragePageState
}) {
  const {
    backendKind,
    canBrowseLocalFs,
    canWrite,
    onSubmitStorage,
    preloadArtifactFolderPicker,
    setArtifactDirPickerOpen,
    settings,
    settingsQuery,
    storageForm,
    updateStorageMutation,
  } = state
  return (
    <section aria-label="Artifact storage configuration" className="space-y-4">
      {settingsQuery.isLoading ? (
        <div className="space-y-3 border bg-card p-4">
          <Skeleton className="h-10 w-full" />
          <Skeleton className="h-10 w-full" />
          <Skeleton className="h-10 w-full" />
        </div>
      ) : null}

      {settingsQuery.error ? (
        <Alert variant="destructive">
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <span>
              Failed to load artifact settings: {settingsQuery.error.message}
            </span>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => void settingsQuery.refetch()}
            >
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      ) : null}

      {!settingsQuery.isLoading && !settingsQuery.error && !settings ? (
        <Alert variant="destructive">
          <AlertDescription className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <span>The response did not include artifact storage settings.</span>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => void settingsQuery.refetch()}
            >
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      ) : null}

      {!settingsQuery.isLoading && !settingsQuery.error && settings ? (
        <div className="border bg-card">
          <div className="border-b px-4 py-3">
            <h2 className="text-sm font-semibold">Storage provider</h2>
            <p className="mt-0.5 text-xs text-muted-foreground">
              Files remain unavailable when storage is disabled.
            </p>
          </div>
          <div className="space-y-4 p-4">
            {!canWrite ? (
              <Alert>
                <AlertDescription>
                  You have read access only. Owner/Admin write permission is
                  required to update settings.
                </AlertDescription>
              </Alert>
            ) : null}

            <Form {...storageForm}>
              <form
                onSubmit={storageForm.handleSubmit(onSubmitStorage)}
                className="space-y-4"
              >
                <FormField
                  control={storageForm.control}
                  name="backend_kind"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Backend</FormLabel>
                      <Select
                        value={field.value}
                        onValueChange={field.onChange}
                        disabled={!canWrite || updateStorageMutation.isPending}
                        items={{
                          disabled: 'Disabled',
                          local: 'Local filesystem',
                          object: 'Object storage (S3-compatible)',
                        }}
                      >
                        <FormControl>
                          <SelectTrigger>
                            <SelectValue placeholder="Select backend" />
                          </SelectTrigger>
                        </FormControl>
                        <SelectContent>
                          <SelectItem value="disabled">Disabled</SelectItem>
                          <SelectItem value="local">
                            Local filesystem
                          </SelectItem>
                          <SelectItem value="object">
                            Object storage (S3-compatible)
                          </SelectItem>
                        </SelectContent>
                      </Select>
                      <FormDescription>
                        {backendKind === 'disabled'
                          ? 'Artifacts are recorded as metadata only. Files are not stored, so downloads are unavailable.'
                          : backendKind === 'local'
                            ? 'Artifacts are uploaded to the daemon and stored on local disk (best for single-host setups).'
                            : 'Artifacts are stored in an S3-compatible bucket and served via time-limited download links.'}
                      </FormDescription>
                      <FormMessage />
                    </FormItem>
                  )}
                />

                {backendKind === 'local' ? (
                  <FormField
                    control={storageForm.control}
                    name="local_base_dir"
                    render={({ field }) => (
                      <FormItem>
                        <FormLabel>Local base directory</FormLabel>
                        <div className="flex flex-col gap-2 md:flex-row">
                          <FormControl>
                            <Input
                              placeholder="/absolute/path/to/artifacts"
                              className="font-mono text-xs"
                              {...field}
                              disabled={
                                !canWrite || updateStorageMutation.isPending
                              }
                            />
                          </FormControl>
                          {canBrowseLocalFs ? (
                            <div className="flex gap-2">
                              <Button
                                type="button"
                                variant="outline"
                                size="icon"
                                aria-label="Browse local base directory"
                                title="Browse local base directory"
                                onMouseEnter={() =>
                                  void preloadArtifactFolderPicker()
                                }
                                onFocus={() =>
                                  void preloadArtifactFolderPicker()
                                }
                                onClick={() => {
                                  setArtifactDirPickerOpen(true)
                                }}
                                disabled={
                                  !canWrite || updateStorageMutation.isPending
                                }
                              >
                                <HugeiconsIcon icon={Folder02Icon} size={16} />
                              </Button>
                            </div>
                          ) : null}
                        </div>
                        <FormDescription>
                          Absolute path on the daemon host where artifact files
                          are stored.
                          {!canBrowseLocalFs ? (
                            <>
                              {' '}
                              For security, folder browsing is only available
                              from localhost.
                            </>
                          ) : null}
                        </FormDescription>
                        <FormMessage />
                      </FormItem>
                    )}
                  />
                ) : null}

                {backendKind === 'object' ? (
                  <ArtifactObjectStorageFields state={state} />
                ) : null}

                <div className="flex justify-end">
                  <Button
                    type="submit"
                    disabled={!canWrite || updateStorageMutation.isPending}
                  >
                    {updateStorageMutation.isPending ? (
                      <>
                        <Spinner className="size-4" />
                        Saving...
                      </>
                    ) : (
                      'Save'
                    )}
                  </Button>
                </div>
              </form>
            </Form>
          </div>
        </div>
      ) : null}
    </section>
  )
}
