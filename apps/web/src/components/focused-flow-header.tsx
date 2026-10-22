/**
 * The header every focused flow (sign-in, setup, sign-in callback) opens with,
 * so moving between them never changes the page's anchor.
 */
export function FocusedFlowHeader({
  title,
  description,
}: {
  title: string
  description?: string
}) {
  return (
    <header className="space-y-4 text-center">
      <img src="/logo.svg" alt="" className="mx-auto size-12" />
      <div className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">{title}</h1>
        {description ? (
          <p className="text-sm text-balance text-muted-foreground">
            {description}
          </p>
        ) : null}
      </div>
    </header>
  )
}
