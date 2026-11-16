import { createOpenAPIPage } from 'fumadocs-openapi/ui'
import type { MediaAdapter } from 'fumadocs-openapi'
import {
  createCodeUsageGeneratorRegistry,
  type CodeUsageGenerator,
} from 'fumadocs-openapi/requests/generators'
import { registerDefault } from 'fumadocs-openapi/requests/generators/all'

const gitBinaryMediaAdapter = {
  encode: ({ body }) => body as BodyInit,
  generateExample: () => undefined,
} satisfies MediaAdapter

function shellVariable(name: string) {
  return `$${name.toUpperCase().replace(/[^A-Z0-9]/g, '_')}`
}

function singleQuote(value: string) {
  return `'${value.replace(/'/g, `'\\''`)}'`
}

/**
 * curl against your own instance: `$OORE_URL` instead of a sample host,
 * `$OORE_TOKEN` as the bearer token, and shell variables for path values the
 * reader has not filled in yet.
 */
export const ooreCurl: CodeUsageGenerator = {
  label: 'cURL',
  lang: 'bash',
  generate(data) {
    const url = new URL(data.url)
    let pathname = decodeURI(url.pathname)
    for (const [name, param] of Object.entries(data.path ?? {})) {
      const value = String(param.value ?? '')
      if (value === '' || value === 'string') {
        pathname = pathname.replace(`/${value}`, `/${shellVariable(name)}`)
      }
    }
    const target = `$OORE_URL${pathname}${url.search}`
    const lines = [
      data.method.toUpperCase() === 'GET'
        ? `curl -fsS "${target}"`
        : `curl -fsS -X ${data.method.toUpperCase()} "${target}"`,
    ]
    const headers = Object.entries(data.header ?? {})
    const authorization = headers.find(
      ([name]) => name.toLowerCase() === 'authorization',
    )
    // The playground fills `Bearer ` with an empty token until the reader
    // types one; show the variable instead.
    const typedToken = String(authorization?.[1].value ?? '')
      .replace(/^Bearer\s*/i, '')
      .trim()
    lines.push(
      typedToken && typedToken !== '<token>'
        ? `-H "Authorization: Bearer ${typedToken}"`
        : '-H "Authorization: Bearer $OORE_TOKEN"',
    )
    for (const [name, param] of headers) {
      if (name.toLowerCase() === 'authorization') continue
      lines.push(`-H "${name}: ${String(param.value)}"`)
    }
    if (data.body !== undefined && data.bodyMediaType) {
      lines.push(`-H "Content-Type: ${data.bodyMediaType}"`)
      if (data.bodyMediaType.includes('json')) {
        const body =
          typeof data.body === 'string'
            ? data.body
            : JSON.stringify(data.body, null, 2)
        lines.push(`-d ${singleQuote(body)}`)
      } else {
        lines.push('--data-binary @body')
      }
    }
    return lines.join(' \\\n  ')
  },
}

const codeUsages = registerDefault(createCodeUsageGeneratorRegistry())
codeUsages.add('curl', ooreCurl)

export const OpenAPIPage = createOpenAPIPage({
  codeUsages,
  mediaAdapters: {
    'application/x-git-upload-pack-advertisement': gitBinaryMediaAdapter,
    'application/x-git-upload-pack-request': gitBinaryMediaAdapter,
    'application/x-git-upload-pack-result': gitBinaryMediaAdapter,
  },
})
