import { HttpResponse, http } from 'msw'

export const telemetryHandlers = [
  http.post(
    /\/v1\/telemetry\/web-performance$/,
    () => new HttpResponse(null, { status: 204 }),
  ),
]
