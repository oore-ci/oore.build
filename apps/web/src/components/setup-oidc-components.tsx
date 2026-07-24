import { useState } from 'react'
import { Copy as Copy01Icon, Check as Tick02Icon } from 'lucide-react'

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
          {copied ? <Tick02Icon className="text-primary" /> : <Copy01Icon />}
        </InputGroupButton>
      </InputGroupAddon>
    </InputGroup>
  )
}
