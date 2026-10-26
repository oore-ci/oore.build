import { mergeProps } from '@base-ui/react/merge-props'
import { useRender } from '@base-ui/react/use-render'

import { stackVariants, type StackVariantProps } from '@/lib/recipes'
import { cn } from '@/lib/utils'

export function Stack({
  className,
  fill,
  gap,
  render,
  ...props
}: useRender.ComponentProps<'div'> & StackVariantProps) {
  return useRender({
    defaultTagName: 'div',
    props: mergeProps<'div'>(
      { className: cn(stackVariants({ fill, gap }), className) },
      props,
    ),
    render,
  })
}
