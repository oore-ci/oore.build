import { cn } from '@/lib/utils'

/**
 * The full-height shell of a focused flow (sign-in, setup, sign-in callback).
 * It owns the fill, so routes never repeat `min-h-0 flex-1`. `top` stacks the
 * flow from the top; `center` centers a single card vertically.
 */
export function FocusedFlowLayout({
  align = 'top',
  className,
  ...props
}: React.ComponentProps<'div'> & { align?: 'top' | 'center' }) {
  return (
    <div
      className={cn(
        'focused-flow flex min-h-0 flex-1 items-center p-4 sm:p-6',
        align === 'top' && 'flex-col',
        className,
      )}
      {...props}
    />
  )
}
