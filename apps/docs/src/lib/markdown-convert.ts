// Turns the authored MDX of a docs page into plain Markdown that agents and
// other tools can read without running the site. Kept free of Astro imports so
// it can be tested with `bun test`.

export const docsOrigin = 'https://docs.oore.build'

const calloutLabels: Record<string, string> = {
  error: 'Danger',
  idea: 'Tip',
  info: 'Note',
  success: 'Note',
  warn: 'Warning',
  warning: 'Warning',
}

function attribute(source: string, name: string): string | undefined {
  const match = new RegExp(
    `${name}\\s*=\\s*(?:"([^"]*)"|'([^']*)'|\\{\\s*["'\`]([^"'\`]*)["'\`]\\s*\\})`,
  ).exec(source)
  return match ? (match[1] ?? match[2] ?? match[3]) : undefined
}

function collapse(text: string) {
  return text.replace(/\s+/g, ' ').trim()
}

export function markdownUrl(pathname: string) {
  const clean = pathname.replace(/\/+$/, '')
  return `${docsOrigin}${clean === '' ? '/index' : clean}.md`
}

function absoluteLink(target: string) {
  if (!target.startsWith('/') || target.startsWith('//')) return target
  return `${docsOrigin}${target}`
}

/** Replace site-relative link and image targets with absolute URLs. */
function absoluteLinks(markdown: string) {
  return markdown.replace(
    /(!?\[[^\]]*\]\()(\/[^)\s]*)((?:\s+"[^"]*")?\))/g,
    (_, start: string, target: string, end: string) =>
      `${start}${absoluteLink(target)}${end}`,
  )
}

function convertCallouts(markdown: string) {
  return markdown.replace(
    /<Callout\b([^>]*)>([\s\S]*?)<\/Callout>/g,
    (_, attributes: string, body: string) => {
      const type = attribute(attributes, 'type') ?? 'info'
      const title = attribute(attributes, 'title')
      const label = calloutLabels[type] ?? 'Note'
      const heading = title ? `**${label}: ${title}**` : `**${label}**`
      const lines = [heading, '', ...body.trim().split('\n')]
      return lines.map((line) => (line.trim() ? `> ${line}` : '>')).join('\n')
    },
  )
}

function convertCards(markdown: string) {
  const withCards = markdown.replace(
    /<Card\b([\s\S]*?)>([\s\S]*?)<\/Card>/g,
    (_, attributes: string, body: string) => {
      const title = attribute(attributes, 'title') ?? 'Link'
      const href = attribute(attributes, 'href')
      const text = collapse(body)
      const link = href ? `[${title}](${absoluteLink(href)})` : title
      return text ? `- ${link}: ${text}` : `- ${link}`
    },
  )
  return withCards.replace(/^[ \t]*<\/?Cards\b[^>]*>[ \t]*\n?/gm, '')
}

function stripModuleLines(markdown: string) {
  return markdown.replace(/^(?:import|export)\s.*$\n?/gm, '')
}

/**
 * Convert one authored page to Markdown. Code fences are left untouched.
 */
export function mdxToMarkdown(input: {
  body: string
  description?: string
  title: string
  url: string
}) {
  const segments = input.body.split(/(^```[\s\S]*?^```[ \t]*$)/m)
  const converted = segments
    .map((segment) =>
      segment.startsWith('```')
        ? segment
        : absoluteLinks(
            convertCards(convertCallouts(stripModuleLines(segment))),
          ),
    )
    .join('')
    .replace(/^[ \t]+(- \[)/gm, '$1')
    .replace(/\n{3,}/g, '\n\n')
    .trim()

  const header = [`# ${input.title}`, '']
  if (input.description) header.push(`> ${collapse(input.description)}`, '')
  header.push(`Source: ${docsOrigin}${input.url === '/' ? '/' : input.url}`, '')
  return `${header.join('\n')}\n${converted}\n`
}
