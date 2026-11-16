import type { APIRoute } from 'astro'

import { llmsTxt } from '@/lib/markdown'

export const prerender = true

export const GET: APIRoute = () =>
  new Response(llmsTxt(), {
    headers: { 'Content-Type': 'text/plain; charset=utf-8' },
  })
