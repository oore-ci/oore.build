import type { APIRoute } from 'astro'

import { llmsFullTxt } from '@/lib/markdown'

export const prerender = true

export const GET: APIRoute = () =>
  new Response(llmsFullTxt(), {
    headers: { 'Content-Type': 'text/plain; charset=utf-8' },
  })
