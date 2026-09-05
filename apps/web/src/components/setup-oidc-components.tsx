import { useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { Copy01Icon, Tick02Icon } from '@hugeicons/core-free-icons'

import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
} from '@/components/ui/input-group'

export function CopyableOidcRedirectUri({ uri }: { uri: string }) {
  const [copied, setCopied] = useState(false)

  function handleCopy() {
    void navigator.clipboard.writeText(uri).then(() => {
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    })
  }

  return (
    <InputGroup>
      <InputGroupInput
        value={uri}
        readOnly
        aria-label="OIDC redirect URI"
        className="font-mono text-xs"
      />
      <InputGroupAddon align="inline-end">
        <InputGroupButton
          type="button"
          variant="ghost"
          size="icon-xs"
          onClick={handleCopy}
          aria-label="Copy redirect URI"
        >
          {copied ? (
            <HugeiconsIcon icon={Tick02Icon} className="text-foreground" />
          ) : (
            <HugeiconsIcon icon={Copy01Icon} />
          )}
        </InputGroupButton>
      </InputGroupAddon>
    </InputGroup>
  )
}
