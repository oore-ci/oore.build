import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useRouter } from '@tanstack/react-router'
import type { InviteUserRequest, UpdateUserRoleRequest } from '@/lib/types'
import {
  deleteUser,
  inviteUser,
  listUsers,
  logout,
  reEnableUser,
  updateUserRole,
} from '@/lib/api'
import { useAuthStore } from '@/stores/auth-store'
import { useApiContext } from '@/hooks/use-api-context'

export function useUsers() {
  const { baseUrl, instance, token } = useApiContext()

  return useQuery({
    queryKey: [instance?.id ?? '__none__', 'users'],
    queryFn: ({ signal }) => listUsers(baseUrl!, token!, { signal }),
    enabled: !!baseUrl && !!token,
  })
}

export function useInviteUser() {
  const queryClient = useQueryClient()
  const { baseUrl, instance, token } = useApiContext()

  return useMutation({
    mutationFn: (data: InviteUserRequest) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return inviteUser(baseUrl, token, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'users'],
      })
    },
  })
}

export function useUpdateUserRole() {
  const queryClient = useQueryClient()
  const { baseUrl, instance, token } = useApiContext()

  return useMutation({
    mutationFn: ({
      userId,
      data,
    }: {
      userId: string
      data: UpdateUserRoleRequest
    }) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return updateUserRole(baseUrl, token, userId, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'users'],
      })
    },
  })
}

export function useReEnableUser() {
  const queryClient = useQueryClient()
  const { baseUrl, instance, token } = useApiContext()

  return useMutation({
    mutationFn: (userId: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return reEnableUser(baseUrl, token, userId)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'users'],
      })
    },
  })
}

export function useDeleteUser() {
  const queryClient = useQueryClient()
  const { baseUrl, instance, token } = useApiContext()

  return useMutation({
    mutationFn: (userId: string) => {
      if (!baseUrl || !token)
        return Promise.reject(new Error('Not authenticated'))
      return deleteUser(baseUrl, token, userId)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: [instance?.id ?? '__none__', 'users'],
      })
    },
  })
}

export function useLogout() {
  const queryClient = useQueryClient()
  const router = useRouter()
  const { baseUrl, token } = useApiContext()
  const clearAuth = useAuthStore((s) => s.clearAuth)

  return useMutation({
    mutationFn: () => {
      if (!baseUrl || !token)
        return Promise.resolve({ ok: true } as { ok: boolean })
      return logout(baseUrl, token)
    },
    onSettled: () => {
      clearAuth()
      queryClient.clear()
      void router.navigate({ to: '/login', replace: true })
    },
  })
}
