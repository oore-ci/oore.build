import type { ReactNode } from 'react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { useAllPipelines, useDeletePipeline } from './use-pipelines'
import { useAllProjects, useDeleteProject } from './use-projects'

const mocks = vi.hoisted(() => ({
  deletePipeline: vi.fn(),
  deleteProject: vi.fn(),
  listAllPipelines: vi.fn(),
  listAllProjects: vi.fn(),
}))

vi.mock('@/lib/api', async (importOriginal) => {
  const actual = await importOriginal()
  return {
    ...(actual as Record<string, unknown>),
    deletePipeline: mocks.deletePipeline,
    deleteProject: mocks.deleteProject,
    listAllPipelines: mocks.listAllPipelines,
    listAllProjects: mocks.listAllProjects,
  }
})

vi.mock('@/stores/instance-store', () => ({
  useActiveInstance: () => ({ id: 'instance-1' }),
}))

vi.mock('@/lib/instance-url', () => ({
  resolveInstanceApiBaseUrl: () => 'https://ci.example.com',
}))

vi.mock('@/stores/auth-store', () => ({
  useAuthStore: (
    selector: (state: { token: string; expiresAt: number }) => unknown,
  ) => selector({ token: 'session-token', expiresAt: 4_102_444_800 }),
}))

function queryWrapper(client: QueryClient) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={client}>{children}</QueryClientProvider>
  }
}

describe('aggregate query invalidation', () => {
  beforeEach(() => {
    mocks.deletePipeline.mockReset()
    mocks.deleteProject.mockReset()
    mocks.listAllPipelines.mockReset()
    mocks.listAllProjects.mockReset()
  })

  it('removes a deleted project from an active all-projects selector', async () => {
    let projects = [{ id: 'project-1', current_user_role: 'maintainer' }]
    mocks.listAllProjects.mockImplementation(() =>
      Promise.resolve({ projects, total: projects.length }),
    )
    mocks.deleteProject.mockImplementation(() => {
      projects = []
      return Promise.resolve()
    })
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    })

    const { result } = renderHook(
      () => ({
        projects: useAllProjects(),
        removeProject: useDeleteProject(),
      }),
      { wrapper: queryWrapper(client) },
    )

    await waitFor(() =>
      expect(result.current.projects.data?.projects).toHaveLength(1),
    )
    await act(async () => {
      await result.current.removeProject.mutateAsync('project-1')
    })

    await waitFor(() =>
      expect(result.current.projects.data?.projects).toHaveLength(0),
    )
    expect(mocks.listAllProjects).toHaveBeenCalledTimes(2)
  })

  it('removes a deleted pipeline from an active all-pipelines selector', async () => {
    let pipelines = [{ id: 'pipeline-1' }]
    mocks.listAllPipelines.mockImplementation(() =>
      Promise.resolve({ pipelines, total: pipelines.length }),
    )
    mocks.deletePipeline.mockImplementation(() => {
      pipelines = []
      return Promise.resolve()
    })
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    })

    const { result } = renderHook(
      () => ({
        pipelines: useAllPipelines('project-1'),
        removePipeline: useDeletePipeline(),
      }),
      { wrapper: queryWrapper(client) },
    )

    await waitFor(() =>
      expect(result.current.pipelines.data?.pipelines).toHaveLength(1),
    )
    await act(async () => {
      await result.current.removePipeline.mutateAsync('pipeline-1')
    })

    await waitFor(() =>
      expect(result.current.pipelines.data?.pipelines).toHaveLength(0),
    )
    expect(mocks.listAllPipelines).toHaveBeenCalledTimes(2)
  })
})
