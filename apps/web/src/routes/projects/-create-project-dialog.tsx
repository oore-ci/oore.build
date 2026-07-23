import { Link } from '@tanstack/react-router'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import { Folder as Folder02Icon, Link2 as Link04Icon } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Input } from '@/components/ui/input'
import LocalFolderPickerDialog from '@/components/LocalFolderPickerDialog'
import {
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/components/ui/form'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Spinner } from '@/components/ui/spinner'
import RepositoryAvatar from '@/components/repository-avatar'
import { SourceDiscoveryWarning } from '@/components/source-discovery-warning'
import { useCreateProjectDialogState } from './-use-create-project-dialog-state'

interface CreateProjectDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

export default function CreateProjectDialog({
  open: requestedOpen,
  onOpenChange,
}: CreateProjectDialogProps) {
  const dialogState = useCreateProjectDialogState(requestedOpen, onOpenChange)
  const {
    canBrowseLocalFs,
    createMutation,
    form,
    handleOpenChange,
    handleOpenPicker,
    hasRepos,
    isRemoteMode,
    onSubmit,
    open,
    pickerOpen,
    repoItems,
    repos,
    reposError,
    repoFailures,
    reposLoading,
    reposRetrying,
    retryRepos,
    setPickerOpen,
  } = dialogState

  return (
    <>
      <Dialog open={open} onOpenChange={handleOpenChange}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Create project</DialogTitle>
            <DialogDescription>
              {isRemoteMode
                ? "Choose a repository from a connected source. Creating the project trusts its build commands to run with the runner account's macOS permissions."
                : "Choose a repository on this Mac. Creating the project trusts its build commands to run with the runner account's macOS permissions."}
            </DialogDescription>
          </DialogHeader>

          <Form {...form}>
            <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
              {isRemoteMode ? (
                <FormField
                  control={form.control}
                  name="repository_id"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Repository</FormLabel>
                      <SourceDiscoveryWarning
                        failures={repoFailures}
                        isRetrying={reposRetrying}
                        onRetry={() => void retryRepos()}
                      />
                      {reposLoading ? (
                        <div className="flex items-center gap-2 py-2">
                          <Spinner className="size-4" />
                          <span className="text-sm text-muted-foreground">
                            Loading repositories...
                          </span>
                        </div>
                      ) : reposError ? (
                        <Alert variant="destructive">
                          <AlertDescription className="flex items-center justify-between gap-3">
                            <span>
                              Failed to load repositories: {reposError.message}
                            </span>
                            <Button
                              type="button"
                              size="sm"
                              variant="outline"
                              onClick={() => void retryRepos()}
                            >
                              Retry
                            </Button>
                          </AlertDescription>
                        </Alert>
                      ) : hasRepos ? (
                        <Select
                          value={field.value}
                          onValueChange={(value) => {
                            field.onChange(value ?? '')
                            const repository = repos?.find(
                              (repo) => repo.id === value,
                            )
                            if (!repository) return
                            if (!form.getFieldState('name').isDirty) {
                              form.setValue(
                                'name',
                                repository.full_name
                                  .split('/')
                                  .filter(Boolean)
                                  .at(-1) ?? '',
                              )
                            }
                            if (!form.getFieldState('default_branch').isDirty) {
                              form.setValue(
                                'default_branch',
                                repository.default_branch ?? '',
                              )
                            }
                          }}
                          items={repoItems}
                        >
                          <FormControl>
                            <SelectTrigger autoFocus>
                              <SelectValue placeholder="Select a repository..." />
                            </SelectTrigger>
                          </FormControl>
                          <SelectContent>
                            {(repos ?? []).map((repo) => (
                              <SelectItem key={repo.id} value={repo.id}>
                                <RepositoryAvatar
                                  fullName={repo.full_name}
                                  avatarUrl={repo.avatar_url}
                                  repositoryId={repo.id}
                                  provider={repo.provider}
                                />
                                <span>{repo.full_name}</span>
                              </SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
                      ) : (
                        <div className="space-y-3">
                          <FormDescription>
                            No repositories are available. Connect a source and
                            sync its repositories first.
                          </FormDescription>
                          <Button
                            type="button"
                            variant="outline"
                            render={<Link to="/settings/integrations" />}
                            nativeButton={false}
                          >
                            <DynamicLucideIcon icon={Link04Icon} />
                            Connect source
                          </Button>
                        </div>
                      )}
                      <FormMessage />
                    </FormItem>
                  )}
                />
              ) : null}

              <FormField
                control={form.control}
                name="name"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Name</FormLabel>
                    <FormControl>
                      <Input
                        placeholder="My App"
                        autoFocus={!isRemoteMode}
                        {...field}
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <FormField
                control={form.control}
                name="description"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>
                      Description{' '}
                      <span className="font-normal text-muted-foreground">
                        (optional)
                      </span>
                    </FormLabel>
                    <FormControl>
                      <Input
                        placeholder="A brief description of this project"
                        {...field}
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              {!isRemoteMode ? (
                <FormField
                  control={form.control}
                  name="local_repository_path"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Path</FormLabel>
                      <div className="flex flex-col gap-2 md:flex-row">
                        <FormControl>
                          <Input
                            {...field}
                            placeholder="/absolute/path/to/repository"
                            className="font-mono text-xs"
                          />
                        </FormControl>
                        {canBrowseLocalFs ? (
                          <div className="flex gap-2">
                            <Button
                              type="button"
                              variant="outline"
                              size="icon"
                              aria-label="Browse"
                              title="Browse"
                              onClick={handleOpenPicker}
                            >
                              <DynamicLucideIcon icon={Folder02Icon} />
                            </Button>
                          </div>
                        ) : null}
                      </div>
                      <FormDescription>
                        Absolute path to the Git repository.
                        {!canBrowseLocalFs ? (
                          <>
                            {' '}
                            For security, folder browsing is only available from
                            localhost.
                          </>
                        ) : null}
                      </FormDescription>
                      <FormMessage />
                    </FormItem>
                  )}
                />
              ) : null}

              <FormField
                control={form.control}
                name="default_branch"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>
                      Default branch{' '}
                      <span className="font-normal text-muted-foreground">
                        (optional)
                      </span>
                    </FormLabel>
                    <FormControl>
                      <Input placeholder="main" {...field} />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <DialogFooter>
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => handleOpenChange(false)}
                >
                  Cancel
                </Button>
                <Button
                  type="submit"
                  disabled={
                    createMutation.isPending ||
                    (isRemoteMode &&
                      (reposLoading || !!reposError || !hasRepos))
                  }
                >
                  {createMutation.isPending ? (
                    <>
                      <Spinner className="size-4" />
                      Creating...
                    </>
                  ) : (
                    'Create project'
                  )}
                </Button>
              </DialogFooter>
            </form>
          </Form>
        </DialogContent>
      </Dialog>

      <LocalFolderPickerDialog
        open={pickerOpen}
        onOpenChange={setPickerOpen}
        enabled={open && !isRemoteMode && canBrowseLocalFs}
        initialPath={form.getValues('local_repository_path')}
        title="Browse Local Folders"
        description="Select a Git repository folder and use it for this project."
        requireGitRepository
        selectCurrentLabel="Use Current Folder"
        selectDirectoryLabel="Use Repo"
        onSelectPath={(path) => {
          form.setValue('local_repository_path', path, {
            shouldDirty: true,
            shouldTouch: true,
            shouldValidate: true,
          })
          setPickerOpen(false)
        }}
      />
    </>
  )
}
