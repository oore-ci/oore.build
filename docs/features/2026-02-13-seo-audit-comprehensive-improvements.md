# Comprehensive SEO Audit and Improvements

## Status

`ready`

## Problem

A full SEO audit of all three user-facing web properties (React SPA at ci.oore.build, VitePress docs at docs.oore.build, and landing page at oore.build) revealed multiple issues: missing sitemaps, relative OG/Twitter image URLs that fail on social platforms, missing canonical/og:url tags, no structured data, no per-page meta descriptions on docs, missing security headers on the landing site, and use of deprecated `document.title` assignment instead of React 19 native meta hoisting.

## User Impact

- Search engines can now discover and index all public pages via proper sitemaps
- Social sharing (Twitter, Facebook, LinkedIn, Slack) renders branded 1200x630 OG images with the actual oore.build logo
- Per-route `<title>` and `<meta description>` tags improve search result appearance
- Authenticated routes (builds, settings, projects) include `noindex` to prevent leaking private content to search engines
- VitePress docs pages all have frontmatter descriptions for better search snippets
- JSON-LD structured data (SoftwareApplication, BreadcrumbList) enables rich results

## UI Changes

- No visual UI changes to the web app or docs site
- Route titles now set via React 19 `<title>` hoisting (PageMeta component) instead of `document.title`
- OG images now use the actual oore.build logo (ring-cutout design) instead of a placeholder

## API Changes

None.

## Security Considerations

- `robots.txt` explicitly blocks `/auth/` and `/settings/` routes from crawler indexing
- Authenticated routes emit `<meta name="robots" content="noindex, nofollow" />` via PageMeta
- No authentication, authorization, or secret-handling changes

## Migration and Rollout

- No backend migration required
- Web app sitemap auto-generates at build time via Vite plugin (`sitemapPlugin`)
- VitePress sitemap auto-generates at build time via `sitemap.hostname` config
- Landing site has static sitemap (single-page site)
- Changes take effect on deploy; social platform caches may take 24-48h to refresh OG images

## Acceptance Criteria

- [x] All three apps have absolute OG/Twitter image URLs pointing to og-image.svg
- [x] og-image.svg uses the actual logo.svg design (ring-cutout, not placeholder)
- [x] Web app auto-generates sitemap.xml at build time via Vite plugin
- [x] VitePress auto-generates sitemap via `sitemap.hostname` config
- [x] Landing site has robots.txt, sitemap.xml, OG tags, JSON-LD, and security headers
- [x] All three apps include canonical and og:url tags
- [x] Web app + landing site have JSON-LD SoftwareApplication schema
- [x] Docs site injects BreadcrumbList JSON-LD per page
- [x] All 78+ docs pages have frontmatter descriptions
- [x] 21 route files migrated from document.title to React 19 PageMeta component
- [x] Authenticated routes include noindex meta tag
- [x] seo.ts renamed to seo.tsx for JSX support
- [x] `make validate` passes (all builds + docs check + cargo check)

## Owner

Platform team

## Last Updated

`2026-02-13`
