import { mergeProps } from '@base-ui/react/merge-props'
import { useRender } from '@base-ui/react/use-render'

import { rowVariants, type RowVariantProps } from '@/lib/recipes'
import { cn } from '@/lib/utils'

export function Row({
  className,
  gap,
  render,
  ...props
}: useRender.ComponentProps<'div'> & RowVariantProps) {
  return useRender({
    defaultTagName: 'div',
    props: mergeProps<'div'>(
      { className: cn(rowVariants({ gap }), className) },
      props,
    ),
    render,
  })
}
