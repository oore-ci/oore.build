import { HttpResponse, delay, http } from 'msw'
import { ago } from '../seed'

export const artifactHandlers = [
  http.post('/v1/artifacts/:artifactId/download-link', async () => {
    await delay(200)
    return HttpResponse.json({
      download_url: '#demo-download',
      expires_at: ago(-3600), // 1 hour from now
    })
  }),
]
