export async function isManagedFrontend(
  fetcher: typeof fetch = fetch,
): Promise<boolean> {
  try {
    const response = await fetcher('/healthz', {
      method: 'GET',
      cache: 'no-store',
    })
    return response.ok && response.headers.get('x-oore-web-proxy') === '1'
  } catch {
    return false
  }
}
