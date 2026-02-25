import { Match, Switch, createEffect, createMemo, createSignal } from 'solid-js'
import { createFileRoute, useNavigate } from '@tanstack/solid-router'
import type { BuildPlatform } from '@/lib/types'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'
import { usePipeline, useUpdatePipeline } from '@/hooks/use-pipelines'
import { PageMeta } from '@/lib/seo'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { FormError, FormField } from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import { PageHeader } from '@/components/page-header'
import { PageLayout } from '@/components/page-layout'
import { Skeleton } from '@/components/ui/skeleton'
import { toast } from '@/components/ui/sonner'

export const Route = createFileRoute('/projects/$projectId/pipelines/$pipelineId_/edit')({
  staticData: { breadcrumbLabel: 'Edit Pipeline' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: EditPipelinePage,
})

function EditPipelinePage() {
  const params = Route.useParams()
  const navigate = useNavigate()
  const pipelineQuery = usePipeline(params().pipelineId)
  const updatePipeline = useUpdatePipeline()

  const [seeded, setSeeded] = createSignal(false)
  const [error, setError] = createSignal<string | null>(null)

  const [name, setName] = createSignal('')
  const [configMode, setConfigMode] = createSignal<'auto' | 'explicit'>('auto')
  const [configPath, setConfigPath] = createSignal('')
  const [platformAndroid, setPlatformAndroid] = createSignal(true)
  const [platformIos, setPlatformIos] = createSignal(false)
  const [platformMacos, setPlatformMacos] = createSignal(false)
  const [flutterVersion, setFlutterVersion] = createSignal('')
  const [enabled, setEnabled] = createSignal(true)

  const pipeline = () => pipelineQuery.data?.pipeline
  const pageTitle = () =>
    pipeline()?.name ? `Edit: ${pipeline()?.name}` : 'Edit Pipeline'

  createEffect(() => {
    const source = pipeline()
    if (!source || seeded()) return

    setName(source.name)
    setConfigMode(source.config_path_explicit ? 'explicit' : 'auto')
    setConfigPath(source.config_path)
    setPlatformAndroid(source.execution_config.platforms.includes('android'))
    setPlatformIos(source.execution_config.platforms.includes('ios'))
    setPlatformMacos(source.execution_config.platforms.includes('macos'))
    setFlutterVersion(source.execution_config.flutter_version ?? '')
    setEnabled(source.enabled)
    setSeeded(true)
  })

  const selectedPlatforms = createMemo<Array<BuildPlatform>>(() => {
    const next: Array<BuildPlatform> = []
    if (platformAndroid()) next.push('android')
    if (platformIos()) next.push('ios')
    if (platformMacos()) next.push('macos')
    return next
  })

  const canSubmit = () => {
    if (!name().trim()) return false
    if (selectedPlatforms().length === 0) return false
    if (configMode() === 'explicit' && !configPath().trim()) return false
    return true
  }

  const handleSave = () => {
    const source = pipeline()
    if (!source) return

    if (!name().trim()) {
      setError('Pipeline name is required.')
      return
    }

    if (selectedPlatforms().length === 0) {
      setError('Select at least one platform.')
      return
    }

    if (configMode() === 'explicit' && !configPath().trim()) {
      setError('Config path is required when using explicit mode.')
      return
    }

    setError(null)

    updatePipeline.mutate(
      {
        pipelineId: source.id,
        data: {
          name: name().trim(),
          config_path: configMode() === 'explicit' ? configPath().trim() : '.oore/pipeline.huml',
          config_path_explicit: configMode() === 'explicit',
          enabled: enabled(),
          execution_config: {
            ...source.execution_config,
            flutter_version: flutterVersion().trim() || undefined,
            platforms: selectedPlatforms(),
          },
          trigger_config: source.trigger_config,
          concurrency: source.concurrency,
        },
      },
      {
        onSuccess: () => {
          toast.success('Pipeline updated')
          void navigate({
            to: '/projects/$projectId/pipelines/$pipelineId',
            params: {
              projectId: params().projectId,
              pipelineId: params().pipelineId,
            },
          })
        },
        onError: (mutationError) => {
          setError(
            mutationError instanceof Error
              ? mutationError.message
              : 'Failed to update pipeline',
          )
        },
      },
    )
  }

  return (
    <PageLayout width="wide" class="space-y-4 pb-20">
      <PageMeta title={pageTitle()} noindex />

      <Switch>
        <Match when={pipelineQuery.isLoading}>
          <Skeleton class="h-8 w-56" />
          <Skeleton class="h-96 w-full" />
        </Match>

        <Match when={!!pipelineQuery.error || !pipeline()}>
          <Alert variant="destructive">
            <AlertDescription>
              Failed to load pipeline: {pipelineQuery.error?.message ?? 'Not found'}
            </AlertDescription>
          </Alert>
        </Match>

        <Match when>
          <PageHeader
            title={pageTitle()}
            back={{ to: `/projects/${params().projectId}/pipelines/${params().pipelineId}`, label: 'Pipeline' }}
            description="Update pipeline configuration."
          />

          <Card>
            <CardContent>
              <FormField label="Pipeline name">
                <Input
                  value={name()}
                  onInput={(event) => setName(event.currentTarget.value)}
                  placeholder="Android Release"
                />
              </FormField>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>Configuration</CardTitle>
            </CardHeader>
            <CardContent class="space-y-4">
              <FormField label="Config source">
                <select
                  class="h-9 w-full border border-input bg-background px-2.5 text-sm"
                  value={configMode()}
                  onChange={(event) =>
                    setConfigMode(event.currentTarget.value as 'auto' | 'explicit')
                  }
                >
                  <option value="auto">Use repo config if found (.oore.yaml, .oore.yml)</option>
                  <option value="explicit">Use a specific config file path</option>
                </select>
              </FormField>

              {configMode() === 'explicit' ? (
                <FormField label="Config path">
                  <Input
                    value={configPath()}
                    onInput={(event) => setConfigPath(event.currentTarget.value)}
                    placeholder=".oore/android-release.yaml"
                  />
                </FormField>
              ) : null}

              <div class="space-y-2">
                <p class="text-xs font-medium text-muted-foreground">Build for platforms</p>
                <div class="grid grid-cols-1 gap-2 sm:grid-cols-3">
                  <label class="flex items-center gap-2 text-sm">
                    <input
                      type="checkbox"
                      checked={platformAndroid()}
                      onChange={(event) => setPlatformAndroid(event.currentTarget.checked)}
                    />
                    Android
                  </label>
                  <label class="flex items-center gap-2 text-sm">
                    <input
                      type="checkbox"
                      checked={platformIos()}
                      onChange={(event) => setPlatformIos(event.currentTarget.checked)}
                    />
                    iOS
                  </label>
                  <label class="flex items-center gap-2 text-sm">
                    <input
                      type="checkbox"
                      checked={platformMacos()}
                      onChange={(event) => setPlatformMacos(event.currentTarget.checked)}
                    />
                    macOS
                  </label>
                </div>
              </div>

              <FormField label="Flutter version (optional)">
                <Input
                  value={flutterVersion()}
                  onInput={(event) => setFlutterVersion(event.currentTarget.value)}
                  placeholder="3.24.3"
                />
                <p class="text-xs text-muted-foreground">
                  If set, runner executes Flutter commands using fvm use {'<version>'}.
                </p>
              </FormField>

              <div class="rounded border bg-muted/30 p-3">
                <p class="mb-1 text-xs font-medium text-muted-foreground">Default build commands</p>
                <p class="font-mono text-xs">• flutter build apk --release --split-per-abi</p>
              </div>
            </CardContent>
          </Card>

          <details class="border bg-card" open>
            <summary class="cursor-pointer px-4 py-3 text-sm font-medium">Triggers</summary>
            <div class="space-y-2 border-t px-4 py-3 text-sm text-muted-foreground">
              Manual / API / webhook trigger behavior is preserved from existing pipeline config.
            </div>
          </details>

          <details class="border bg-card" open>
            <summary class="cursor-pointer px-4 py-3 text-sm font-medium">Execution config</summary>
            <div class="space-y-2 border-t px-4 py-3 text-sm text-muted-foreground">
              Existing pre-build, build, post-build commands and artifact patterns are preserved.
            </div>
          </details>

          <details class="border bg-card" open>
            <summary class="cursor-pointer px-4 py-3 text-sm font-medium">Android signing</summary>
            <div class="space-y-2 border-t px-4 py-3 text-sm text-muted-foreground">
              Existing signing configuration is preserved.
            </div>
          </details>

          {error() ? <FormError>{error() ?? ''}</FormError> : null}

          <label class="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={enabled()}
              onChange={(event) => setEnabled(event.currentTarget.checked)}
            />
            Pipeline enabled
          </label>

          <div class="fixed inset-x-0 bottom-0 z-40 border-t bg-background/95 px-6 py-3 backdrop-blur">
            <div class="mx-auto flex w-full max-w-6xl items-center justify-end gap-2">
              <Button
                type="button"
                variant="outline"
                onClick={() =>
                  void navigate({
                    to: '/projects/$projectId/pipelines/$pipelineId',
                    params: {
                      projectId: params().projectId,
                      pipelineId: params().pipelineId,
                    },
                  })
                }
              >
                Cancel
              </Button>
              <Button
                type="button"
                variant="outline"
                onClick={() => {
                  if (!canSubmit()) {
                    setError('Form has validation issues. Please review required fields.')
                    return
                  }
                  setError(null)
                  toast.success('Validation passed')
                }}
              >
                Validate
              </Button>
              <Button
                type="button"
                onClick={handleSave}
                disabled={updatePipeline.isPending || !canSubmit()}
              >
                {updatePipeline.isPending ? 'Saving...' : 'Save'}
              </Button>
            </div>
          </div>
        </Match>
      </Switch>
    </PageLayout>
  )
}
