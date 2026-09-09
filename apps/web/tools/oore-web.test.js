import assert from 'node:assert/strict'
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { gunzipSync } from 'node:zlib'
import { test } from 'bun:test'
import { spaFileResponse } from './oore-web.js'

test('static assets negotiate gzip without changing bytes, type or cache policy', async () => {
  const directory = mkdtempSync(join(tmpdir(), 'oore-compression-'))
  try {
    const file = join(directory, 'app.js')
    const source = 'console.log("oore");\n'.repeat(100)
    writeFileSync(file, source)
    for (const encoding of ['gzip', 'br, gzip;q=0.5', '*;q=1']) {
      const response = spaFileResponse(file, '/assets/app.js', encoding)
      assert.equal(response.headers.get('Content-Encoding'), 'gzip')
      assert.equal(response.headers.get('Vary'), 'Accept-Encoding')
      assert.match(response.headers.get('Content-Type'), /javascript/)
      assert.match(response.headers.get('Cache-Control'), /immutable/)
      const bytes = await response.arrayBuffer()
      assert.ok(bytes.byteLength < source.length)
      assert.equal(gunzipSync(bytes).toString(), source)
    }
    for (const encoding of ['', 'br', 'gzip;q=0, *;q=1', 'gzip;q=invalid']) {
      const response = spaFileResponse(file, '/assets/app.js', encoding)
      assert.equal(response.headers.get('Content-Encoding'), null)
      assert.equal(await response.text(), source)
    }
  } finally {
    rmSync(directory, { recursive: true })
  }
})
