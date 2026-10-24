import type {
  AppStorePublishConfig,
  ExportPipelineYamlRequest,
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
