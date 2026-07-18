import { cn } from '@/lib/utils'

const WIDTH_CLASSES = {
  default: 'max-w-5xl',
  narrow: 'max-w-2xl',
  wide: 'max-w-7xl',
  full: '',
} as const

interface PageLayoutProps {
  children: React.ReactNode
  width?: keyof typeof WIDTH_CLASSES
  className?: string
}

export default function PageLayout({
  children,
  width = 'default',
  className,
}: PageLayoutProps) {
  return (
    <div
      className={cn(
        WIDTH_CLASSES[width],
        'mx-auto min-w-0 w-full space-y-5 px-4 py-5 sm:px-6 sm:py-6 lg:px-8 lg:py-8',
        className,
      )}
    >
      {children}
    </div>
  )
}
