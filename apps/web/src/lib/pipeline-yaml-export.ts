import type {
  AppStorePublishConfig,
  ExportPipelineYamlRequest,
  ExportPipelineYamlResponse,
  GooglePlayPublishConfig,
} from '@oore/client/models'
import type { PipelineFormValues } from '@/lib/pipeline-schema'
import {
  executionConfigFromForm,
  triggerConfigFromForm,
} from '@/lib/pipeline-form-utils'

/** The pipeline's Publish settings, as the publish endpoints return them. */
export interface PipelinePublishSettings {
  googlePlay?: GooglePlayPublishConfig
  appStore?: AppStorePublishConfig
}

/** Form settings with a half-typed build number offset read as none. */
export function planConfigFromForm(values: PipelineFormValues) {
  const config = executionConfigFromForm(values)
  return {
    ...config,
    build_number_offset: Number.isFinite(config.build_number_offset)
      ? config.build_number_offset
      : 0,
  }
}

/**
 * The export request for the form. Triggers are left out for repositories
 * that only build manually, so the file has no `on` there. Only enabled
 * Publish settings are sent; the daemon copies their track, rollout, draft
 * and TestFlight group into the file, never their credentials.
 */
export function yamlExportRequest(
  values: PipelineFormValues,
  {
    manualOnlyTriggers,
    publish,
  }: { manualOnlyTriggers: boolean; publish?: PipelinePublishSettings },
): ExportPipelineYamlRequest {
  const request: ExportPipelineYamlRequest = {
    execution_config: planConfigFromForm(values),
  }
  if (!manualOnlyTriggers) {
    request.trigger_config = triggerConfigFromForm(values, false)
  }
  if (publish?.googlePlay?.enabled) request.google_play = publish.googlePlay
  if (publish?.appStore?.enabled) request.app_store = publish.appStore
  return request
}

/** Where the runner looks for the file: the explicit path, else `.oore.yaml`. */
export function repositoryFilePath(values: PipelineFormValues): string {
  const explicit =
    values.config_mode === 'explicit' ? values.config_path?.trim() : ''
  return explicit || '.oore.yaml'
}

/** The name a downloaded copy gets: the file name without its folders. */
export function downloadFileName(path: string): string {
  return path.split('/').filter(Boolean).at(-1) ?? '.oore.yaml'
}

/** Where another pipeline in the project reads its repository file from. */
export interface PipelineConfigSource {
  name: string
  config_path: string
  config_path_explicit: boolean
}

/**
 * The other pipelines that would read a file committed at `path`: those with
 * that explicit path, and, for the default names, those that look for
 * `.oore.yaml` or `.oore.yml` themselves.
 */
export function pipelinesReadingPath(
  path: string,
  others: ReadonlyArray<PipelineConfigSource>,
): Array<string> {
  const defaultName = path === '.oore.yaml' || path === '.oore.yml'
  return others
    .filter((other) =>
      other.config_path_explicit
        ? other.config_path.trim() === path
        : defaultName,
    )
    .map((other) => other.name)
}

/**
 * What the Repository file pane shows. A request error wins over a stale
 * file, and a file the daemon could not generate shows its errors, never an
 * earlier file.
 */
export function repositoryFileView({
  isError,
  result,
}: {
  isError: boolean
  result: ExportPipelineYamlResponse | undefined
}):
  | { kind: 'error' }
  | { kind: 'loading' }
  | { kind: 'invalid'; errors: Array<string> }
  | { kind: 'ready'; yaml: string; warnings: Array<string> } {
  if (isError) return { kind: 'error' }
  if (!result) return { kind: 'loading' }
  if (!result.valid || !result.yaml) {
    return {
      kind: 'invalid',
      errors:
        result.errors.length > 0
          ? result.errors
          : ['The daemon returned no file for these settings.'],
    }
  }
  return { kind: 'ready', yaml: result.yaml, warnings: result.warnings }
}
