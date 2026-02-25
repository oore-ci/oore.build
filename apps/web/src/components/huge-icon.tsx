import { For, type Component, type JSX } from 'solid-js'
import type { HugeIconNode } from '@/lib/instance-icons'
import { cn } from '@/lib/utils'

interface HugeIconProps extends JSX.SvgSVGAttributes<SVGSVGElement> {
  icon: HugeIconNode
  size?: number
  class?: string
}

export const HugeIcon: Component<HugeIconProps> = (props) => {
  const size = () => props.size ?? 16
  const { icon, class: className, size: _size, ...rest } = props

  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      width={size()}
      height={size()}
      fill="none"
      class={cn('shrink-0', className)}
      aria-hidden="true"
      {...rest}
    >
      <For each={icon}>
        {(node) => {
          const [tag, attrs] = node
          const svgAttrs = attrs as Record<string, string | number>
          return tag === 'path' ? <path {...svgAttrs} /> : null
        }}
      </For>
    </svg>
  )
}
