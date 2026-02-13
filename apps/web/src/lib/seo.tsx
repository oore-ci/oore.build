export const WEB_BRAND_NAME = 'Oore CI'
export const WEB_BRAND_TAGLINE = 'Self-Hosted Mobile CI'
export const WEB_BRAND_DESCRIPTION =
  'Self-hosted, Flutter-first mobile CI and internal app distribution platform'

export function webPageTitle(page?: string): string {
  if (!page) return `${WEB_BRAND_NAME} | ${WEB_BRAND_TAGLINE}`
  return `${page} | ${WEB_BRAND_NAME}`
}

/**
 * React 19 native `<title>` and `<meta>` hoisting component.
 *
 * Renders document metadata tags that React 19 automatically hoists to
 * `<head>`. Use this inside any route component instead of manual
 * `document.title` assignment.
 *
 * Note: social crawlers (Twitter, Facebook, LinkedIn) do NOT execute JS,
 * so OG tags here only benefit Google. Static OG tags in index.html cover
 * the social sharing case.
 */
export function PageMeta({
  title,
  description,
  noindex,
}: {
  title?: string
  description?: string
  noindex?: boolean
}) {
  const pageTitle = webPageTitle(title)
  const desc = description ?? WEB_BRAND_DESCRIPTION

  return (
    <>
      <title>{pageTitle}</title>
      <meta name="description" content={desc} />
      {noindex && <meta name="robots" content="noindex, nofollow" />}
    </>
  )
}
