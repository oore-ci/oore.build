import {
  createOoreClient,
  OoreApiError,
  type OoreClient,
} from '@oore/client/client'
import {
  skipToken,
  type InfiniteData,
  type QueryFunction,
  type QueryFunctionContext,
  type QueryKey,
  type UseInfiniteQueryOptions,
  type UseQueryOptions,
} from '@tanstack/react-query'

import { READ_ONLY_REASON, isDemoMutationBlocked } from '@/lib/demo-mode'
const appFetch: typeof fetch = async (input, init) => {
  const request = new Request(input, init)
  const method = request.method.toUpperCase()
  const path = new URL(request.url).pathname

  if (isDemoMutationBlocked(method, path)) {
    throw new OoreApiError(READ_ONLY_REASON, {
      code: 'demo_read_only',
      request,
      response: new Response(null, { status: 403 }),
    })
  }

  return globalThis.fetch(request)
}

export function createWebOoreClient({
  baseUrl,
  token,
}: {
  baseUrl: string
  token?: string
}): OoreClient {
  return createOoreClient({ baseUrl, fetch: appFetch, token })
}

export function scopeOoreQueryKey(
  instanceId: string,
  queryKey: QueryKey,
): QueryKey {
  return [instanceId, ...queryKey]
}

interface GeneratedQueryOptions<
  TData,
  TQueryKey extends QueryKey,
  TPageParam = never,
> {
  queryFn?: QueryFunction<TData, TQueryKey, TPageParam> | typeof skipToken
  queryKey: TQueryKey
}

export function scopeOoreQueryOptions<TData, TQueryKey extends QueryKey>(
  instanceId: string,
  generatedOptions: GeneratedQueryOptions<TData, TQueryKey>,
): UseQueryOptions<TData, Error, TData, QueryKey> {
  const { queryFn, queryKey } = generatedOptions
  const scopedQueryKey = scopeOoreQueryKey(instanceId, queryKey)

  if (queryFn === undefined || queryFn === skipToken) {
    return { queryFn, queryKey: scopedQueryKey }
  }

  return {
    queryKey: scopedQueryKey,
    queryFn: (context: QueryFunctionContext<QueryKey>) =>
      queryFn({
        client: context.client,
        direction: context.direction,
        meta: context.meta,
        pageParam: context.pageParam,
        queryKey,
        signal: context.signal,
      }),
  }
}

export function scopeOoreInfiniteQueryOptions<
  TData,
  TQueryKey extends QueryKey,
  TPageParam,
>(
  instanceId: string,
  generatedOptions: GeneratedQueryOptions<TData, TQueryKey, TPageParam>,
): Omit<
  UseInfiniteQueryOptions<
    TData,
    Error,
    InfiniteData<TData>,
    QueryKey,
    TPageParam
  >,
  'getNextPageParam' | 'initialPageParam'
> {
  const { queryFn, queryKey } = generatedOptions
  const scopedQueryKey = scopeOoreQueryKey(instanceId, queryKey)

  if (queryFn === undefined || queryFn === skipToken) {
    return { queryFn, queryKey: scopedQueryKey }
  }

  return {
    queryKey: scopedQueryKey,
    queryFn: (context: QueryFunctionContext<QueryKey, TPageParam>) => {
      // SAFETY: The wrapper changes only the key; TanStack supplies every other field with the same page parameter type.
      const generatedContext = {
        ...context,
        queryKey,
      } as QueryFunctionContext<TQueryKey, TPageParam>
      return queryFn(generatedContext)
    },
  }
}
