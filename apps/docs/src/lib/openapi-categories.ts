import type {
  OperationItem,
  PagesBuilder,
  SchemaToPagesOptions,
} from 'fumadocs-openapi'

const safeOperationId = /^(?!\.{1,2}$)[A-Za-z0-9._~-]+$/

export const OPENAPI_CATEGORIES = [
  {
    slug: 'authentication',
    title: 'Authentication',
    tags: ['Auth', 'API Tokens'],
  },
  { slug: 'builds', title: 'Builds', tags: ['Builds'] },
  { slug: 'releases', title: 'Releases', tags: ['Releases', 'Environments'] },
  { slug: 'users', title: 'Users', tags: ['Users'] },
  {
    slug: 'projects',
    title: 'Projects',
    tags: ['Projects', 'Project Members'],
  },
  {
    slug: 'artifacts',
    title: 'Artifacts',
    tags: ['Artifacts', 'Scoped Download Tokens'],
  },
  {
    slug: 'sources',
    title: 'Sources',
    tags: ['Integrations', 'Webhooks'],
  },
  {
    slug: 'pipelines',
    title: 'Pipelines',
    tags: [
      'Pipelines',
      'Pipeline Signing',
      'Pipeline Publishing',
      'Store Credentials',
      'Secrets',
    ],
  },
  { slug: 'setup', title: 'Setup', tags: ['Health', 'Setup'] },
  {
    slug: 'logs',
    title: 'Logs',
    tags: ['Build Logs', 'Events', 'Audit Logs'],
  },
  {
    slug: 'settings',
    title: 'Settings',
    tags: [
      'Instance Settings',
      'Retention Policy',
      'Notification Channels',
      'System',
    ],
  },
  { slug: 'runners', title: 'Runners', tags: ['Runners'] },
] as const

function categoryForTags(tags: string[]) {
  const matches = OPENAPI_CATEGORIES.filter((category) =>
    category.tags.some((tag) => tags.includes(tag)),
  )
  if (matches.length !== 1) {
    throw new Error(
      `OpenAPI tags must map to exactly one category: ${tags.join(', ') || '(none)'}`,
    )
  }
  return matches[0]
}

function createCategoryPages(builder: PagesBuilder) {
  const extracted = builder.extract()
  const usedOperationIds = new Set<string>()
  const usedTags = new Set<string>()
  const operations = new Map<string, OperationItem[]>(
    OPENAPI_CATEGORIES.map((category) => [category.slug, []]),
  )

  for (const item of extracted.operations) {
    const tags = item.tags ?? []
    for (const tag of tags) usedTags.add(tag)

    const resolved = builder.fromExtractedOperation(item)
    const operationId = resolved?.operation.operationId
    if (
      typeof operationId !== 'string' ||
      !safeOperationId.test(operationId) ||
      usedOperationIds.has(operationId)
    ) {
      throw new Error(
        `OpenAPI operation must have a unique URL-safe operationId: ${item.method.toUpperCase()} ${item.path}`,
      )
    }
    usedOperationIds.add(operationId)

    operations.get(categoryForTags(tags).slug)!.push({
      method: item.method,
      path: item.path,
    })
  }

  for (const tag of usedTags) {
    const matches = OPENAPI_CATEGORIES.filter((category) =>
      category.tags.some((mappedTag) => mappedTag === tag),
    )
    if (matches.length !== 1 || !builder.fromTagName(tag)) {
      throw new Error(
        `Used OpenAPI tag must be declared and map to exactly one category: ${tag}`,
      )
    }
  }

  for (const category of OPENAPI_CATEGORIES) {
    const items = operations.get(category.slug)!
    if (items.length === 0) {
      throw new Error(`OpenAPI category must not be empty: ${category.title}`)
    }
    for (const tag of category.tags) {
      if (!usedTags.has(tag)) {
        throw new Error(`Mapped OpenAPI tag is unused: ${tag}`)
      }
    }

    const description = category.tags
      .map((tag) => builder.fromTagName(tag)?.info.description)
      .filter((value): value is string => Boolean(value))
      .join('\n\n')

    builder.create({
      type: 'page',
      path: `${category.slug}.mdx`,
      schemaId: builder.id,
      info: {
        title: category.title,
        description: description || undefined,
      },
      operations: items,
      webhooks: [],
    })
  }
}

export const openAPICategoryPages = {
  per: 'custom',
  toPages: createCategoryPages,
} satisfies SchemaToPagesOptions
