import { createMemo, createSignal } from 'solid-js'
import { createFileRoute, useNavigate } from '@tanstack/solid-router'
import type { BuildPlatform } from '@/lib/types'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { FormError, FormField } from '@/components/ui/form'
import { Input } from '@/components/ui/input'
import { PageHeader } from '@/components/page-header'
import { PageLayout } from '@/components/page-layout'
import { PageMeta } from '@/lib/seo'
import { Textarea } from '@/components/ui/textarea'
import { useCreatePipeline } from '@/hooks/use-pipelines'
import { toast } from '@/components/ui/sonner'
import {
  getActiveInstanceOrRedirect,
  requireAuthOrRedirect,
} from '@/lib/instance-context'

export const Route = createFileRoute('/projects/$projectId/pipelines/new')({
  staticData: { breadcrumbLabel: 'New Pipeline' },
  beforeLoad: () => {
    const instance = getActiveInstanceOrRedirect()
    requireAuthOrRedirect(instance.id)
  },
  component: NewPipelinePage,
})

function NewPipelinePage() {
  const params = Route.useParams()
  const navigate = useNavigate()
  const createPipeline = useCreatePipeline(params().projectId)

  const [error, setError] = createSignal<string | null>(null)

  const [name, setName] = createSignal('Release')
  const [configMode, setConfigMode] = createSignal<'auto' | 'explicit'>('auto')
  const [configPath, setConfigPath] = createSignal('.oore/pipeline.huml')
  const [platformAndroid, setPlatformAndroid] = createSignal(true)
  const [platformIos, setPlatformIos] = createSignal(false)
  const [platformMacos, setPlatformMacos] = createSignal(false)
  const [flutterVersion, setFlutterVersion] = createSignal('3.24.0')
  const [triggerEvents, setTriggerEvents] = createSignal('manual')
  const [branches, setBranches] = createSignal('main')
  const [description, setDescription] = createSignal('')

  const platforms = createMemo<Array<BuildPlatform>>(() => {
    const next: Array<BuildPlatform> = []
    if (platformAndroid()) next.push('android')
    if (platformIos()) next.push('ios')
    if (platformMacos()) next.push('macos')
    return next
  })

  const canSubmit = () => {
    if (!name().trim()) return false
    if (platforms().length === 0) return false
    if (configMode() === 'explicit' && !configPath().trim()) return false
    return true
  }

  const handleCreate = async () => {
    if (!canSubmit()) {
      setError('Please complete required fields before creating the pipeline.')
      return
    }

    setError(null)

    try {
      await createPipeline.mutateAsync({
        name: name().trim(),
        config_path:
          configMode() === 'explicit'
            ? configPath().trim()
            : '.oore/pipeline.huml',
        config_path_explicit: configMode() === 'explicit',
        trigger_config: {
          events: triggerEvents()
            .split(',')
            .map((value) => value.trim())
            .filter(Boolean),
          branches: branches()
            .split(',')
            .map((value) => value.trim())
            .filter(Boolean),
        },
        concurrency: {
          cancel_previous: false,
          max_concurrent: 1,
        },
        execution_config: {
          platforms: platforms(),
          flutter_version: flutterVersion().trim() || undefined,
          commands: {
            pre_build: [],
            build: [],
            post_build: [],
          },
          artifact_patterns: [],
          env: description().trim()
            ? [
                {
                  key: 'PIPELINE_DESCRIPTION',
                  value: description().trim(),
                },
              ]
            : [],
        },
      })

      void navigate({
        to: '/projects/$projectId',
        params: { projectId: params().projectId },
      })
    } catch (submitError) {
      setError(
        submitError instanceof Error
          ? submitError.message
          : 'Pipeline creation failed',
      )
    }
  }

  const handleValidate = () => {
    if (!canSubmit()) {
      setError('Form has validation issues. Please review required fields.')
      return
    }

    setError(null)
    toast.success('Validation passed')
  }

  return (
    <PageLayout width="wide" class="space-y-4 pb-20">
      <PageMeta title="New Pipeline" noindex />
      <PageHeader
        title="New Pipeline"
        back={{ to: `/projects/${params().projectId}`, label: 'Project' }}
        description="Configure a new build pipeline for this project."
      />

      {error() ? (
        <Alert variant="destructive">
          <AlertTitle>Unable to create pipeline</AlertTitle>
          <AlertDescription>{error()}</AlertDescription>
        </Alert>
      ) : null}

      <Card>
        <CardContent>
          <FormField label="Pipeline name">
            <Input
              value={name()}
              onInput={(event) => setName(event.currentTarget.value)}
              placeholder="Release"
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
              placeholder="3.24.0 (or stable)"
            />
          </FormField>

          <div class="rounded border bg-muted/30 p-3">
            <p class="mb-1 text-xs font-medium text-muted-foreground">Default build commands</p>
            <p class="font-mono text-xs">• flutter build apk --release</p>
          </div>
        </CardContent>
      </Card>

      <details class="border bg-card" open>
        <summary class="cursor-pointer px-4 py-3 text-sm font-medium">Triggers</summary>
        <div class="space-y-4 border-t px-4 py-3">
          <FormField label="Trigger events (comma-separated)">
            <Input
              value={triggerEvents()}
              onInput={(event) => setTriggerEvents(event.currentTarget.value)}
              placeholder="manual, webhook"
            />
          </FormField>
          <FormField label="Branches (comma-separated)">
            <Input
              value={branches()}
              onInput={(event) => setBranches(event.currentTarget.value)}
              placeholder="main, release/*"
            />
          </FormField>
        </div>
      </details>

      <details class="border bg-card" open>
        <summary class="cursor-pointer px-4 py-3 text-sm font-medium">Metadata</summary>
        <div class="space-y-4 border-t px-4 py-3">
          <FormField label="Description (optional)">
            <Textarea
              value={description()}
              onInput={(event) => setDescription(event.currentTarget.value)}
              placeholder="Optional description"
            />
          </FormField>
        </div>
      </details>

      {!canSubmit() ? (
        <FormError>
          Provide a pipeline name and at least one platform before creating.
        </FormError>
      ) : null}

      <div class="fixed inset-x-0 bottom-0 z-40 border-t bg-background/95 px-6 py-3 backdrop-blur">
        <div class="mx-auto flex w-full max-w-6xl items-center justify-end gap-2">
          <Button
            type="button"
            variant="outline"
            onClick={() =>
              void navigate({
                to: '/projects/$projectId',
                params: { projectId: params().projectId },
              })
            }
          >
            Cancel
          </Button>
          <Button
            type="button"
            variant="outline"
            onClick={handleValidate}
          >
            Validate
          </Button>
          <Button
            type="button"
            onClick={handleCreate}
            disabled={createPipeline.isPending || !canSubmit()}
          >
            {createPipeline.isPending ? 'Creating...' : 'Create'}
          </Button>
        </div>
      </div>
    </PageLayout>
  )
}
