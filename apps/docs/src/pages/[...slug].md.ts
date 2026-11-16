import type { APIRoute, GetStaticPaths } from 'astro'

import { markdownHeaders, pageMarkdown } from '@/lib/markdown'
import { source } from '@/lib/source'

export const prerender = true

// Every docs page as Markdown at `<path>.md`. The home page is `/index.md`.
export const getStaticPaths = (() =>
  source.getPages().map((page) => ({
    params: { slug: page.slugs.length > 0 ? page.slugs.join('/') : 'index' },
  }))) satisfies GetStaticPaths

export const GET: APIRoute = ({ params }) => {
  const slug = params.slug === 'index' ? [] : (params.slug ?? '').split('/')
  const page = source.getPage(slug)
  if (!page) return new Response('Not found', { status: 404 })
  return new Response(pageMarkdown(page), { headers: markdownHeaders })
}
