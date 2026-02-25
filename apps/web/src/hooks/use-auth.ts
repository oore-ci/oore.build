import { createMutation, createQuery, useQueryClient } from '@tanstack/solid-query'
import { useNavigate } from '@tanstack/solid-router'
import type { InviteUserRequest, UpdateUserRoleRequest } from '@/lib/types'
import {
  deleteUser,
  getMe,
  inviteUser,
  listUsers,
  logout,
  reEnableUser,
  updateUserRole,
} from '@/lib/api'
import { useAuthToken, useBaseUrl, useInstanceQueryPrefix } from '@/hooks/query-context'
import { useAuthStore } from '@/stores/auth-store'

export function useCurrentUser() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'me'],
    queryFn: () => getMe(baseUrl()!, token()!),
    enabled: !!baseUrl() && !!token(),
  }))
}

export function useUsers() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const prefix = useInstanceQueryPrefix()

  return createQuery(() => ({
    queryKey: [prefix(), 'users'],
    queryFn: () => listUsers(baseUrl()!, token()!),
    enabled: !!baseUrl() && !!token(),
  }))
}

export function useInviteUser() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const queryClient = useQueryClient()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async (data: InviteUserRequest) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return inviteUser(baseUrl()!, token()!, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: [prefix(), 'users'] })
    },
  }))
}

export function useUpdateUserRole() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const queryClient = useQueryClient()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async ({
      userId,
      data,
    }: {
      userId: string
      data: UpdateUserRoleRequest
    }) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return updateUserRole(baseUrl()!, token()!, userId, data)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: [prefix(), 'users'] })
    },
  }))
}

export function useReEnableUser() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const queryClient = useQueryClient()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async (userId: string) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return reEnableUser(baseUrl()!, token()!, userId)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: [prefix(), 'users'] })
    },
  }))
}

export function useDeleteUser() {
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const queryClient = useQueryClient()
  const prefix = useInstanceQueryPrefix()

  return createMutation(() => ({
    mutationFn: async (userId: string) => {
      if (!baseUrl() || !token()) throw new Error('Not authenticated')
      return deleteUser(baseUrl()!, token()!, userId)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: [prefix(), 'users'] })
    },
  }))
}

export function useLogout() {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const baseUrl = useBaseUrl()
  const token = useAuthToken()
  const clearAuth = useAuthStore((state) => state.clearAuth)

  return createMutation(() => ({
    mutationFn: async () => {
      if (!baseUrl() || !token()) return { ok: true }
      return logout(baseUrl()!, token()!)
    },
    onSettled: () => {
      clearAuth()()
      queryClient.clear()
      void navigate({ to: '/login', replace: true })
    },
  }))
}
