import { describe, expect, it } from 'vitest'
import { addInstanceSchema } from '@/components/AddInstanceDialog'

describe('addInstanceSchema', () => {
  it('allows local empty backend url', () => {
    const schema = addInstanceSchema('http://localhost:3000')
    const result = schema.safeParse({
      label: 'Local Dev',
      url: '',
      icon: 'cloud-server',
    })

    expect(result.success).toBe(true)
  })

  it('rejects non-http(s) backend url', () => {
    const schema = addInstanceSchema('http://localhost:3000')
    const result = schema.safeParse({
      label: 'Local Dev',
      url: 'ftp://example.com',
      icon: 'cloud-server',
    })

    expect(result.success).toBe(false)
  })

  it('rejects http backend url on hosted ui origin', () => {
    const schema = addInstanceSchema('https://ci.oore.build')
    const result = schema.safeParse({
      label: 'Hosted',
      url: 'http://127.0.0.1:8787',
      icon: 'cloud-server',
    })

    expect(result.success).toBe(false)
    if (result.success) return
    expect(result.error.issues[0]?.message).toContain(
      'Hosted UI requires an HTTPS backend URL',
    )
  })

  it('rejects empty backend url on hosted ui origin', () => {
    const schema = addInstanceSchema('https://ci.oore.build')
    const result = schema.safeParse({
      label: 'Hosted',
      url: '',
      icon: 'cloud-server',
    })

    expect(result.success).toBe(false)
  })

  it('allows https backend url on hosted ui origin', () => {
    const schema = addInstanceSchema('https://ci.oore.build')
    const result = schema.safeParse({
      label: 'Hosted',
      url: 'https://ci.example.com',
      icon: 'cloud-server',
    })

    expect(result.success).toBe(true)
  })

  it('rejects loopback backend url for local oore-web launcher origin', () => {
    const schema = addInstanceSchema('http://127.0.0.1:4173')
    const result = schema.safeParse({
      label: 'Local Launcher',
      url: 'http://127.0.0.1:8787',
      icon: 'cloud-server',
    })

    expect(result.success).toBe(false)
    if (result.success) return
    expect(result.error.issues[0]?.message).toContain('leave Backend URL empty')
  })

  it('allows empty backend url for local oore-web launcher origin', () => {
    const schema = addInstanceSchema('http://127.0.0.1:4173')
    const result = schema.safeParse({
      label: 'Local Launcher',
      url: '',
      icon: 'cloud-server',
    })

    expect(result.success).toBe(true)
  })
})
