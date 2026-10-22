import { cn } from '@/lib/utils'

export const PAGE_WIDTH_CLASSES = {
  default: 'max-w-5xl',
  narrow: 'max-w-2xl',
  wide: 'max-w-7xl',
  full: '',
} as const

interface PageLayoutProps {
  children: React.ReactNode
  className?: string
  fill?: boolean
  width?: keyof typeof PAGE_WIDTH_CLASSES
}

export default function PageLayout({
  children,
  className,
  fill = false,
  width = 'default',
}: PageLayoutProps) {
  return (
    <div
      className={cn(
        PAGE_WIDTH_CLASSES[width],
        'mx-auto w-full min-w-0 px-4 py-5 sm:p-6 lg:p-8',
        fill ? 'flex min-h-0 flex-1 flex-col gap-5' : 'flex flex-col gap-5',
        className,
      )}
    >
      {children}
    </div>
  )
}
