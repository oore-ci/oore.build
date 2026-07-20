import { useCallback, useState } from 'react'
import { DynamicLucideIcon } from '@/components/ui/dynamic-lucide-icon'
import { Copy as Copy01Icon, Check as Tick02Icon } from 'lucide-react'

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
      <code className="flex-1 font-mono text-xs break-all">{uri}</code>
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        onClick={handleCopy}
        className="shrink-0 text-muted-foreground hover:text-foreground"
        aria-label="Copy redirect URI"
      >
        {copied ? (
          <DynamicLucideIcon
            icon={Tick02Icon}
            size={14}
            className="text-primary"
          />
        ) : (
          <DynamicLucideIcon icon={Copy01Icon} size={14} />
        )}
      </Button>
    </div>
  )
}
