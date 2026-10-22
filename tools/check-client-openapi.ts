import { existsSync } from 'node:fs'
import { readFile } from 'node:fs/promises'
import path from 'node:path'

const HTTP_METHODS = ['delete', 'get', 'head', 'patch', 'post', 'put'] as const

interface OpenApiDocument {
  paths?: Record<string, Record<string, unknown>>
}

export function specOperations(spec: OpenApiDocument) {
  return new Set(
    Object.entries(spec.paths ?? {}).flatMap(([route, operations]) =>
      HTTP_METHODS.filter((method) => method in operations).map(
        (method) => `${method.toUpperCase()} ${route}`,
      ),
    ),
  )
}

// Clients published before the spec shipped in the package only expose their
// generated SDK, where each operation is `client).<method>({ ... url: '<route>'`
// (or `client).sse.<method>(` for event streams).
export function sdkOperations(sdk: string) {
  const pattern = new RegExp(
    String.raw`\)\.(?:sse\.)?(${HTTP_METHODS.join('|')})\(\{[^;]*?\burl: '([^']+)'`,
    'g',
  )
  return new Set(
    Array.from(
      sdk.matchAll(pattern),
      ([, method, route]) => `${method!.toUpperCase()} ${route}`,
    ),
  )
}

export function compareOperations(expected: Set<string>, actual: Set<string>) {
  return {
    missing: [...expected].filter((operation) => !actual.has(operation)).sort(),
    extra: [...actual].filter((operation) => !expected.has(operation)).sort(),
  }
}

async function main() {
  const [specPath, webDir = 'apps/web'] = process.argv.slice(2)
  if (!specPath) {
    throw new Error(
      'usage: bun tools/check-client-openapi.ts <openapi.json> [web-app-dir]',
    )
  }
  const spec = JSON.parse(await readFile(specPath, 'utf8')) as OpenApiDocument
  const clientDir = path.dirname(
    Bun.resolveSync('@oore/client/package.json', path.resolve(webDir)),
  )
  const { version } = JSON.parse(
    await readFile(path.join(clientDir, 'package.json'), 'utf8'),
  ) as { version: string }

  const bundledSpec = path.join(clientDir, 'openapi/openapi.json')
  if (existsSync(bundledSpec)) {
    const clientSpec = JSON.parse(await readFile(bundledSpec, 'utf8'))
    if (Bun.deepEquals(clientSpec, spec, true)) {
      console.log(`@oore/client ${version} matches ${specPath}.`)
      return
    }
    const { missing, extra } = compareOperations(
      specOperations(spec),
      specOperations(clientSpec),
    )
    fail(version, specPath, missing, extra)
  }

  const sdk = await readFile(
    path.join(clientDir, 'dist/generated/sdk.gen.js'),
    'utf8',
  )
  const { missing, extra } = compareOperations(
    specOperations(spec),
    sdkOperations(sdk),
  )
  if (missing.length || extra.length) fail(version, specPath, missing, extra)
  console.log(
    `@oore/client ${version} has every operation in ${specPath}. It does not bundle its spec, so schema changes are not compared.`,
  )
}

function fail(
  version: string,
  specPath: string,
  missing: Array<string>,
  extra: Array<string>,
): never {
  const lines = [
    `@oore/client ${version} was generated from a different spec than ${specPath}.`,
    ...missing.map((operation) => `  missing from the client: ${operation}`),
    ...extra.map((operation) => `  only in the client: ${operation}`),
    'Publish @oore/client from the current spec (oore-ci/oore-client-js), then update apps/web/package.json and bun.lock.',
  ]
  console.error(lines.join('\n'))
  process.exit(1)
}

if (import.meta.main) await main()
