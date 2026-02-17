import { createFileRoute } from '@tanstack/react-router'
import logo from '../logo.svg'

export const Route = createFileRoute('/')({
  component: App,
})

function App() {
  return (
    <div className="text-center">
      <header className="min-h-screen flex flex-col items-center justify-center gap-4 bg-[#171717] text-white p-6">
        <img src={logo} className="h-20 w-20 pointer-events-none" alt="Oore CI logo" />
        <h1 className="text-3xl font-semibold tracking-tight">Oore CI docs</h1>
        <p className="max-w-xl text-base text-neutral-300">
          Documentation shell for local development. Production docs are served by VitePress.
        </p>
      </header>
    </div>
  )
}
