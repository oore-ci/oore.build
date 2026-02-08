export const WEB_BRAND_NAME = 'Oore CI'
export const WEB_BRAND_TAGLINE = 'Self-Hosted Mobile CI'

export function webPageTitle(page?: string): string {
  if (!page) return `${WEB_BRAND_NAME} | ${WEB_BRAND_TAGLINE}`
  return `${page} | ${WEB_BRAND_NAME}`
}
