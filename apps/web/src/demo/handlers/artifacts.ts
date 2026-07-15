import { HttpResponse, delay, http } from 'msw'
import { ago } from '../seed'
import { demoArtifacts } from '../data/artifacts'

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
    const artifact = Object.values(demoArtifacts)
      .flat()
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
]
