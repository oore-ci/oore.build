import { cn } from '@/lib/utils'

const WIDTH_CLASSES = {
  default: 'max-w-4xl',
  narrow: 'max-w-xl',
  wide: 'max-w-6xl',
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
        'mx-auto min-w-0 w-full space-y-6 px-4 py-6 sm:px-6 sm:py-8 lg:px-10 lg:py-10',
        className,
      )}
    >
      {children}
    </div>
  )
}
