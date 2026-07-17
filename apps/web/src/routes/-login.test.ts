import { describe, expect, it } from 'vitest'
import { buildLoginBackendCommands } from './login'

describe('buildLoginBackendCommands', () => {
  it('quotes a crafted backend as one argument in both commands', () => {
    const backendUrl =
      "https://backend.invalid/path/$(SUBSTITUTION);`BACKTICK`'"
    const quotedBackendUrl =
      "'https://backend.invalid/path/$(SUBSTITUTION);`BACKTICK`'\"'\"''"

    expect(buildLoginBackendCommands(backendUrl)).toEqual({
      cloudflared: `cloudflared tunnel --url ${quotedBackendUrl}`,
      ooreWeb: `oore-web --backend-url ${quotedBackendUrl}`,
    })
  })

  it('keeps ordinary backend URLs usable in both commands', () => {
    expect(buildLoginBackendCommands('https://ci.example.com')).toEqual({
      cloudflared: "cloudflared tunnel --url 'https://ci.example.com'",
      ooreWeb: "oore-web --backend-url 'https://ci.example.com'",
    })
  })
})
