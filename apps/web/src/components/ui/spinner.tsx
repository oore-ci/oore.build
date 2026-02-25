import { Loading03Icon } from '@hugeicons/core-free-icons'
import { HugeIcon } from '@/components/huge-icon'
import { cn } from '@/lib/utils'

export function Spinner(props: { class?: string }) {
  return (
    <HugeIcon
      icon={Loading03Icon}
      role="status"
      aria-label="Loading"
      class={cn('size-4 animate-spin', props.class)}
    />
  )
}
