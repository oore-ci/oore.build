import { useRouterState } from '@tanstack/react-router'

export default function RouteTransitionBar() {
  const isLoading = useRouterState({
    select: (state) => state.status === 'pending',
  })
  if (!isLoading) return null
  return (
    <div className="fixed top-0 left-0 right-0 z-50 h-0.5 overflow-hidden bg-primary/20">
      <div className="h-full w-1/3 animate-[route-slide_1s_ease-in-out_infinite] bg-primary" />
    </div>
  )
}
