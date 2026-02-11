import { HttpResponse, delay, http } from 'msw'
import { demoPipelines } from '../data/pipelines'
import { demoBuilds } from '../data/builds'
import { ago } from '../seed'

export const pipelineHandlers = [
  http.get('/v1/projects/:projectId/pipelines', async ({ params }) => {
    await delay(150)
    const pipelines = demoPipelines.filter(
      (p) => p.project_id === params.projectId,
    )
    return HttpResponse.json({ pipelines, total: pipelines.length })
  }),

  http.get('/v1/pipelines/:pipelineId', async ({ params }) => {
    await delay(150)
    const pipeline = demoPipelines.find((p) => p.id === params.pipelineId)
    if (!pipeline) {
      return HttpResponse.json(
        { error: 'Pipeline not found', code: 'not_found' },
        { status: 404 },
      )
    }
    return HttpResponse.json({
      pipeline,
      build_count: demoBuilds.filter((b) => b.pipeline_id === pipeline.id)
        .length,
    })
  }),

  http.post(
    '/v1/projects/:projectId/pipelines',
    async ({ params, request }) => {
      await delay(300)
      const body = (await request.json()) as Record<string, unknown>
      return HttpResponse.json({
        pipeline: {
          id: `pipe-demo-new-${Date.now()}`,
          project_id: params.projectId,
          config_path: '.oore/pipeline.yaml',
          config_path_explicit: false,
          enabled: true,
          created_at: ago(0),
          updated_at: ago(0),
          ...body,
        },
      })
    },
  ),

  http.patch('/v1/pipelines/:pipelineId', async ({ params, request }) => {
    await delay(200)
    const body = (await request.json()) as Record<string, unknown>
    const pipeline = demoPipelines.find((p) => p.id === params.pipelineId)
    return HttpResponse.json({
      pipeline: pipeline
        ? { ...pipeline, ...body, updated_at: ago(0) }
        : { id: params.pipelineId, ...body, updated_at: ago(0) },
    })
  }),

  http.delete('/v1/pipelines/:pipelineId', async () => {
    await delay(200)
    return new HttpResponse(null, { status: 204 })
  }),

  http.post('/v1/pipelines/validate', async () => {
    await delay(200)
    return HttpResponse.json({ valid: true })
  }),

  http.get('/v1/pipelines/:pipelineId/android-signing', async ({ params }) => {
    await delay(150)
    return HttpResponse.json({
      pipeline_id: params.pipelineId,
      debug: {
        build_type: 'debug',
        enabled: false,
        has_keystore: false,
        has_store_password: false,
        has_key_password: false,
      },
      release: {
        build_type: 'release',
        enabled: true,
        has_keystore: true,
        keystore_filename: 'release.keystore',
        keystore_checksum: 'sha256:abc123...',
        key_alias: 'upload',
        has_store_password: true,
        has_key_password: true,
        updated_at: ago(86400 * 10),
      },
    })
  }),

  http.put(
    '/v1/pipelines/:pipelineId/android-signing',
    async ({ params, request }) => {
      await delay(300)
      const body = (await request.json()) as {
        debug?: Record<string, unknown>
        release?: Record<string, unknown>
      }
      return HttpResponse.json({
        pipeline_id: params.pipelineId,
        debug: {
          build_type: 'debug',
          enabled: false,
          has_keystore: false,
          has_store_password: false,
          has_key_password: false,
          ...(body.debug ?? {}),
        },
        release: {
          build_type: 'release',
          enabled: true,
          has_keystore: true,
          keystore_filename: 'release.keystore',
          has_store_password: true,
          has_key_password: true,
          updated_at: ago(0),
          ...(body.release ?? {}),
        },
      })
    },
  ),
]
