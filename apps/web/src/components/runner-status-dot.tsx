export default function RunnerStatusDot({ status }: { status: string }) {
  if (status === 'online' || status === 'busy') {
    return (
      <span className="relative mr-2 inline-flex size-2">
        <span className="absolute inline-flex size-full animate-ping rounded-full bg-success opacity-75" />
        <span className="relative inline-flex size-2 rounded-full bg-success" />
      </span>
    )
  }
  if (status === 'offline') {
    return (
      <span className="mr-2 inline-flex size-2 rounded-full bg-destructive" />
    )
  }
  return <span className="mr-2 inline-flex size-2 rounded-full bg-warning" />
}
