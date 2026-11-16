import type { AstroProviderProps } from 'fumadocs-core/framework/astro'
import type { SerializedPageTree } from 'fumadocs-core/source/client'
import { deserializePageTree } from 'fumadocs-core/source/client'
import type { OpenAPIPageProps } from 'fumadocs-openapi/ui'
import { DocsLayout } from 'fumadocs-ui/layouts/docs'
import {
  DocsBody,
  DocsDescription,
  DocsPage,
  DocsTitle,
  type DocsPageProps,
} from 'fumadocs-ui/layouts/docs/page'
import { RootProvider } from 'fumadocs-ui/provider/astro'
import { useMemo, type ReactNode } from 'react'
import { navigate } from 'astro:transitions/client'

import { OpenAPIPage } from '@/components/api-page'
import StaticSearchDialog from '@/components/search'
import { baseOptions } from '@/lib/layout.shared'

type OpenAPIDocument = {
  description?: string
  operation?: {
    method: string
    operationId: string
    path: string
  }
  props: OpenAPIPageProps
  title: string
}

export function Docs({
  children,
  markdownHref,
  openapi,
  page,
  params,
  pathname,
  tree,
}: {
  children?: ReactNode
  markdownHref?: string
  openapi?: OpenAPIDocument
  page?: DocsPageProps
  params: AstroProviderProps['params']
  pathname: string
  tree: SerializedPageTree
}) {
  // `deserializePageTree()` mutates its input. Clone the island prop so Astro
  // serializes the original string-only tree after server rendering.
  const loadedTree = useMemo(
    () => deserializePageTree(structuredClone(tree)),
    [tree],
  )

  return (
    <RootProvider
      pathname={pathname}
      params={params}
      navigate={navigate}
      search={{ SearchDialog: StaticSearchDialog }}
      theme={{
        defaultTheme: 'system',
        enableSystem: true,
        storageKey: 'oore-docs-theme',
      }}
    >
      <DocsLayout {...baseOptions()} tree={loadedTree}>
        {openapi ? (
          <DocsPage {...page} full>
            <DocsTitle>{openapi.title}</DocsTitle>
            {openapi.description ? (
              <DocsDescription>{openapi.description}</DocsDescription>
            ) : null}
            <DocsBody>
              {openapi.operation || markdownHref ? (
                <div className="not-prose mb-6 flex flex-wrap items-baseline gap-x-3 gap-y-1 border-b pb-4 text-sm">
                  {openapi.operation ? (
                    <>
                      <span className="text-fd-muted-foreground">
                        Operation ID
                      </span>
                      <code className="font-mono">
                        {openapi.operation.operationId}
                      </code>
                    </>
                  ) : null}
                  {markdownHref ? (
                    <a
                      className="text-fd-muted-foreground hover:text-fd-foreground ms-auto underline-offset-4 hover:underline"
                      href={markdownHref}
                    >
                      View as Markdown
                    </a>
                  ) : null}
                </div>
              ) : null}
              <OpenAPIPage {...openapi.props} />
            </DocsBody>
          </DocsPage>
        ) : (
          <DocsPage {...page}>{children}</DocsPage>
        )}
      </DocsLayout>
    </RootProvider>
  )
}
