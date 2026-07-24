import { HttpResponse, delay, http } from 'msw'
import { ago } from '../seed'
import { demoState } from '../state'

export const artifactHandlers = [
  http.post('/v1/artifacts/:artifactId/download-link', async () => {
    await delay(200)
    return HttpResponse.json({
      download_url: '#demo-download',
      expires_at: ago(-3600), // 1 hour from now
    })
  }),
  http.post('/v1/artifacts/:artifactId/install-link', async ({ params }) => {
    await delay(200)
    const artifact = Object.values(demoState.artifacts)
      .flatMap((artifacts) => artifacts ?? [])
      .find((candidate) => candidate.id === params.artifactId)
    const ios = artifact?.artifact_type === 'ipa'
    return HttpResponse.json({
      platform: ios ? 'ios' : 'android',
      install_url: ios
        ? 'itms-services://?action=download-manifest&url=https%3A%2F%2Fdemo.oore.build%2Fmanifest.plist'
        : '#demo-download',
      download_url: '#demo-download',
      manifest_url: ios ? 'https://demo.oore.build/manifest.plist' : undefined,
      expires_at: ago(-3600),
    })
  }),
  http.post(
    '/v1/artifacts/:artifactId/scoped-token',
    async ({ params, request }) => {
      await delay(200)
      const artifact = Object.values(demoState.artifacts)
        .flatMap((artifacts) => artifacts ?? [])
        .find((candidate) => candidate.id === params.artifactId)
      if (!artifact) {
        return HttpResponse.json(
          { error: 'Artifact not found', code: 'not_found' },
          { status: 404 },
        )
      }
      const body = (await request.json()) as {
        ttl_secs?: number
        single_use?: boolean
      }
      const token = `demo_${crypto.randomUUID().replaceAll('-', '')}`
      return HttpResponse.json({
        id: `artifact-token-${crypto.randomUUID().slice(0, 8)}`,
        download_url: `/v1/artifacts/${artifact.id}/download?token=${token}`,
        token,
        prefix: token.slice(0, 12),
        expires_at: ago(-(body.ttl_secs ?? 86400)),
        single_use: body.single_use ?? false,
      })
    },
  ),
]
