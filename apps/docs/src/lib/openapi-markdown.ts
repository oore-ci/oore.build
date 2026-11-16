// Markdown for the generated API reference: one page per operation and one
// per category. Pure functions over the OpenAPI document so `bun test` can
// cover them.

import { docsOrigin, markdownUrl } from './markdown-convert'

type Json = Record<string, unknown>

export interface OpenAPIDocumentLike {
  components?: { schemas?: Record<string, Json> }
  paths: Record<string, Record<string, unknown>>
}

export interface OperationRef {
  method: string
  operation: Json
  path: string
}

const methods = ['get', 'put', 'post', 'delete', 'patch', 'head', 'options']

function isRecord(value: unknown): value is Json {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function text(value: unknown) {
  return typeof value === 'string' ? value.trim() : ''
}

function cell(value: string) {
  return (
    value
      .replace(/\|/g, '\\|')
      .replace(/\s*\n\s*/g, ' ')
      .trim() || ' '
  )
}

function refName(ref: string) {
  return ref.split('/').pop() ?? ref
}

function resolve(document: OpenAPIDocumentLike, schema: unknown): Json {
  if (!isRecord(schema)) return {}
  if (typeof schema.$ref === 'string') {
    return document.components?.schemas?.[refName(schema.$ref)] ?? {}
  }
  return schema
}

export function schemaType(schema: unknown): string {
  if (!isRecord(schema)) return 'any'
  if (typeof schema.$ref === 'string') return refName(schema.$ref)
  for (const key of ['oneOf', 'anyOf', 'allOf'] as const) {
    const variants = schema[key]
    if (Array.isArray(variants) && variants.length > 0) {
      const names = variants.map(schemaType)
      return names.join(key === 'allOf' ? ' & ' : ' | ')
    }
  }
  if (Array.isArray(schema.enum) && schema.enum.length > 0) {
    return schema.enum.map((value) => JSON.stringify(value)).join(' | ')
  }
  const types = Array.isArray(schema.type)
    ? schema.type.filter((type): type is string => typeof type === 'string')
    : typeof schema.type === 'string'
      ? [schema.type]
      : []
  const names = types.map((type) => {
    if (type === 'array') return `${schemaType(schema.items)}[]`
    if (type === 'object' && isRecord(schema.additionalProperties)) {
      return `map<string, ${schemaType(schema.additionalProperties)}>`
    }
    if (typeof schema.format === 'string' && type !== 'null') {
      return `${type} (${schema.format})`
    }
    return type
  })
  return names.length > 0 ? names.join(' | ') : 'object'
}

/** A small example value that satisfies the schema's required fields. */
export function exampleValue(
  document: OpenAPIDocumentLike,
  schema: unknown,
  depth = 0,
): unknown {
  const resolved = resolve(document, schema)
  if (resolved.example !== undefined) return resolved.example
  if (Array.isArray(resolved.enum) && resolved.enum.length > 0) {
    return resolved.enum[0]
  }
  for (const key of ['oneOf', 'anyOf', 'allOf'] as const) {
    const variants = resolved[key]
    if (Array.isArray(variants) && variants.length > 0) {
      const usable = variants.find(
        (variant) => !(isRecord(variant) && variant.type === 'null'),
      )
      return exampleValue(document, usable ?? variants[0], depth + 1)
    }
  }
  const type = Array.isArray(resolved.type)
    ? resolved.type.find((value) => value !== 'null')
    : resolved.type
  if (depth > 4) return type === 'array' ? [] : type === 'object' ? {} : null
  switch (type) {
    case 'string':
      return '<string>'
    case 'integer':
    case 'number':
      return 0
    case 'boolean':
      return false
    case 'array':
      return []
    default: {
      const properties = isRecord(resolved.properties)
        ? resolved.properties
        : {}
      const required = Array.isArray(resolved.required)
        ? resolved.required.filter(
            (name): name is string => typeof name === 'string',
          )
        : []
      const value: Json = {}
      for (const name of required) {
        value[name] = exampleValue(document, properties[name], depth + 1)
      }
      return value
    }
  }
}

function fieldTable(document: OpenAPIDocumentLike, schema: unknown) {
  const resolved = resolve(document, schema)
  const properties = isRecord(resolved.properties) ? resolved.properties : {}
  const names = Object.keys(properties)
  if (names.length === 0) return []
  const required = new Set(
    Array.isArray(resolved.required) ? resolved.required : [],
  )
  return [
    '| Field | Type | Required | Description |',
    '| --- | --- | --- | --- |',
    ...names.map((name) => {
      const property = properties[name]
      const description = isRecord(property)
        ? text(property.description) ||
          text(resolve(document, property).description)
        : ''
      return `| \`${name}\` | ${cell(schemaType(property))} | ${required.has(name) ? 'yes' : 'no'} | ${cell(description)} |`
    }),
  ]
}

function jsonBody(content: unknown) {
  if (!isRecord(content)) return undefined
  const [mediaType, media] =
    Object.entries(content).find(([type]) => type.includes('json')) ??
    Object.entries(content)[0] ??
    []
  if (!mediaType || !isRecord(media)) return undefined
  return { mediaType, schema: media.schema }
}

function parameters(document: OpenAPIDocumentLike, ref: OperationRef) {
  const pathItem = document.paths[ref.path] ?? {}
  const all = [
    ...(Array.isArray(pathItem.parameters) ? pathItem.parameters : []),
    ...(Array.isArray(ref.operation.parameters)
      ? ref.operation.parameters
      : []),
  ]
  return all.map((parameter) => resolve(document, parameter))
}

export function isProtected(operation: Json) {
  return (
    Array.isArray(operation.security) &&
    operation.security.some(
      (requirement) =>
        isRecord(requirement) && Object.keys(requirement).length > 0,
    )
  )
}

function shellVariable(name: string) {
  return `$${name.toUpperCase().replace(/[^A-Z0-9]/g, '_')}`
}

function singleQuote(value: string) {
  return `'${value.replace(/'/g, `'\\''`)}'`
}

export function curlExample(document: OpenAPIDocumentLike, ref: OperationRef) {
  const query = parameters(document, ref)
    .filter((parameter) => parameter.in === 'query' && parameter.required)
    .map(
      (parameter) =>
        `${String(parameter.name)}=${shellVariable(String(parameter.name))}`,
    )
  const path = ref.path.replace(/\{([^}]+)\}/g, (_, name: string) =>
    shellVariable(name),
  )
  const url = `$OORE_URL${path}${query.length > 0 ? `?${query.join('&')}` : ''}`
  const lines = [
    ref.method === 'get'
      ? `curl -fsS "${url}"`
      : `curl -fsS -X ${ref.method.toUpperCase()} "${url}"`,
  ]
  if (isProtected(ref.operation)) {
    lines.push('-H "Authorization: Bearer $OORE_TOKEN"')
  }
  const requestBody = resolve(document, ref.operation.requestBody)
  const body = jsonBody(requestBody.content)
  if (body?.mediaType.includes('json')) {
    lines.push(`-H "Content-Type: ${body.mediaType}"`)
    const example = JSON.stringify(exampleValue(document, body.schema), null, 2)
    lines.push(`-d ${singleQuote(example)}`)
  } else if (body) {
    lines.push(`-H "Content-Type: ${body.mediaType}"`, '--data-binary @body')
  }
  return lines.join(' \\\n  ')
}

export function operationPageUrl(operationId: string) {
  return `/openapi/operations/${operationId}`
}

export function listOperations(document: OpenAPIDocumentLike): OperationRef[] {
  const result: OperationRef[] = []
  for (const [path, pathItem] of Object.entries(document.paths)) {
    for (const method of methods) {
      const operation = pathItem[method]
      if (isRecord(operation)) result.push({ method, operation, path })
    }
  }
  return result
}

export function operationTitle(ref: OperationRef) {
  return (
    text(ref.operation.summary) ||
    text(ref.operation.operationId) ||
    `${ref.method.toUpperCase()} ${ref.path}`
  )
}

export function operationMarkdown(
  document: OpenAPIDocumentLike,
  ref: OperationRef,
) {
  const operationId = text(ref.operation.operationId)
  const description = text(ref.operation.description)
  const out = [`# ${operationTitle(ref)}`, '']
  out.push(
    `Source: ${docsOrigin}${operationPageUrl(operationId)}`,
    '',
    `- Operation ID: \`${operationId}\``,
    `- Request: \`${ref.method.toUpperCase()} ${ref.path}\``,
    isProtected(ref.operation)
      ? '- Authentication: `Authorization: Bearer <token>` with an API token or a session token. See https://docs.oore.build/team/api-tokens.md'
      : '- Authentication: none. This route is public or uses its own token, state or signature.',
  )
  if (Array.isArray(ref.operation.tags) && ref.operation.tags.length > 0) {
    out.push(`- Tags: ${ref.operation.tags.join(', ')}`)
  }
  out.push('')
  if (description) out.push(description, '')

  const params = parameters(document, ref)
  if (params.length > 0) {
    out.push(
      '## Parameters',
      '',
      '| Name | In | Type | Required | Description |',
      '| --- | --- | --- | --- | --- |',
      ...params.map(
        (parameter) =>
          `| \`${String(parameter.name)}\` | ${String(parameter.in)} | ${cell(schemaType(parameter.schema))} | ${parameter.required ? 'yes' : 'no'} | ${cell(text(parameter.description))} |`,
      ),
      '',
    )
  }

  const requestBody = resolve(document, ref.operation.requestBody)
  const body = jsonBody(requestBody.content)
  if (body) {
    out.push(
      '## Request body',
      '',
      `\`${body.mediaType}\`, ${requestBody.required ? 'required' : 'optional'}, type \`${schemaType(body.schema)}\`.`,
      '',
    )
    const table = fieldTable(document, body.schema)
    if (table.length > 0) out.push(...table, '')
  }

  out.push('## Example', '', '```bash', curlExample(document, ref), '```', '')

  const responses = isRecord(ref.operation.responses)
    ? ref.operation.responses
    : {}
  const entries = Object.entries(responses)
  if (entries.length > 0) {
    out.push(
      '## Responses',
      '',
      '| Status | Description | Body |',
      '| --- | --- | --- |',
    )
    const details: string[] = []
    for (const [status, value] of entries) {
      const response = resolve(document, value)
      const responseBody = jsonBody(response.content)
      out.push(
        `| ${status} | ${cell(text(response.description))} | ${responseBody ? `\`${cell(schemaType(responseBody.schema))}\`` : ' '} |`,
      )
      if (status.startsWith('2') && responseBody) {
        const table = fieldTable(document, responseBody.schema)
        if (table.length > 0) {
          details.push(
            `### ${status} body: \`${schemaType(responseBody.schema)}\``,
            '',
            ...table,
            '',
          )
        }
      }
    }
    out.push('', ...details)
  }

  out.push(
    'Errors use the shared envelope `{"error": "...", "code": "..."}` described at https://docs.oore.build/reference/errors.md. The full contract is https://docs.oore.build/openapi.json.',
    '',
  )
  return out.join('\n')
}

export function operationIndexLine(ref: OperationRef) {
  const operationId = text(ref.operation.operationId)
  return `- [${operationTitle(ref)}](${markdownUrl(operationPageUrl(operationId))}): \`${ref.method.toUpperCase()} ${ref.path}\` (\`${operationId}\`)`
}

export function categoryMarkdown(
  document: OpenAPIDocumentLike,
  category: { slug: string; tags: readonly string[]; title: string },
  description?: string,
) {
  const out = [
    `# ${category.title} API`,
    '',
    `Source: ${docsOrigin}/reference/api/categories/${category.slug}`,
    '',
  ]
  if (description) out.push(description, '')
  for (const tag of category.tags) {
    const operations = listOperations(document).filter(
      (ref) =>
        Array.isArray(ref.operation.tags) && ref.operation.tags.includes(tag),
    )
    if (operations.length === 0) continue
    out.push(`## ${tag}`, '', ...operations.map(operationIndexLine), '')
  }
  return out.join('\n')
}
