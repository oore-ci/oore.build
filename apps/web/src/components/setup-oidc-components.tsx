import { useCallback, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { Copy01Icon, Tick02Icon } from '@hugeicons/core-free-icons'

import { Button } from '@/components/ui/button'

export function CopyableOidcRedirectUri({ uri }: { uri: string }) {
  const [copied, setCopied] = useState(false)

  const handleCopy = useCallback(() => {
    void navigator.clipboard.writeText(uri).then(() => {
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    })
  }, [uri])

  return (
    <div className="flex items-center gap-2 bg-muted px-3 py-2">
      <code className="flex-1 break-all font-mono text-xs">{uri}</code>
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        onClick={handleCopy}
        className="shrink-0 text-muted-foreground hover:text-foreground"
        aria-label="Copy redirect URI"
      >
        {copied ? (
          <HugeiconsIcon icon={Tick02Icon} size={14} className="text-primary" />
        ) : (
          <HugeiconsIcon icon={Copy01Icon} size={14} />
        )}
      </Button>
    </div>
  )
}
