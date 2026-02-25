import { createEffect } from 'solid-js'

export const WEB_BRAND_NAME = 'Oore CI'
export const WEB_BRAND_TAGLINE = 'Self-Hosted Mobile CI'
export const WEB_BRAND_DESCRIPTION =
  'Self-hosted, Flutter-first mobile CI and internal app distribution platform'

export function webPageTitle(page?: string): string {
  if (!page) return `${WEB_BRAND_NAME} | ${WEB_BRAND_TAGLINE}`
  return `${page} | ${WEB_BRAND_NAME}`
}

export function PageMeta({
  title,
  description,
}: {
  title?: string
  description?: string
  noindex?: boolean
}) {
  createEffect(() => {
    document.title = webPageTitle(title)

    const desc = description ?? WEB_BRAND_DESCRIPTION
    const meta = document.querySelector('meta[name="description"]')
    if (meta) {
      meta.setAttribute('content', desc)
    }
  })

  return null
}
