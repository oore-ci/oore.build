import type { toast as sonnerToast } from 'sonner'

type SonnerToast = typeof sonnerToast

const loadToast = () => import('sonner').then(({ toast }) => toast)

export const toast = {
  error: (...args: Parameters<SonnerToast['error']>) => {
    void loadToast().then((sonner) => sonner.error(...args))
  },
  message: (...args: Parameters<SonnerToast['message']>) => {
    void loadToast().then((sonner) => sonner.message(...args))
  },
  success: (...args: Parameters<SonnerToast['success']>) => {
    void loadToast().then((sonner) => sonner.success(...args))
  },
  warning: (...args: Parameters<SonnerToast['warning']>) => {
    void loadToast().then((sonner) => sonner.warning(...args))
  },
}
