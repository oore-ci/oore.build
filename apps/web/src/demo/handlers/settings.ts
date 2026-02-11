import { HttpResponse, delay, http } from 'msw'
import {
  demoArtifactStorageSettings,
  demoInstancePreferences,
} from '../data/settings'
import { ago } from '../seed'

export const settingsHandlers = [
  http.get('/v1/settings/artifact-storage', async () => {
    await delay(150)
    return HttpResponse.json({ settings: demoArtifactStorageSettings })
  }),

  http.put('/v1/settings/artifact-storage', async ({ request }) => {
    await delay(300)
    const body = (await request.json()) as Record<string, unknown>
    return HttpResponse.json({
      settings: {
        ...demoArtifactStorageSettings,
        ...body,
        updated_at: ago(0),
      },
    })
  }),

  http.get('/v1/settings/preferences', async () => {
    await delay(150)
    return HttpResponse.json({ preferences: demoInstancePreferences })
  }),

  http.put('/v1/settings/preferences', async ({ request }) => {
    await delay(300)
    const body = (await request.json()) as Record<string, unknown>
    return HttpResponse.json({
      preferences: {
        ...demoInstancePreferences,
        ...body,
        updated_at: ago(0),
      },
    })
  }),
]
