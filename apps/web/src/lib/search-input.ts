import * as z from 'zod'

export type SearchValue = boolean | null | number | string | undefined

export interface SearchInput {
  readonly [key: string]: SearchValue
}

function searchValue(search: SearchInput, key: string) {
  return Object.hasOwn(search, key) ? search[key] : undefined
}

export function searchString(
  search: SearchInput,
  key: string,
): string | undefined {
  return z.string().safeParse(searchValue(search, key)).data
}

export function searchNumber(search: SearchInput, key: string): number {
  const value = searchValue(search, key)
  const number = z.coerce.number().safeParse(value)
  return number.success && Number.isFinite(number.data)
    ? number.data
    : Number.NaN
}

export function searchChoice<T extends string>(
  search: SearchInput,
  key: string,
  choices: ReadonlySet<T> | ReadonlyArray<T>,
): T | undefined {
  const value = searchString(search, key)
  for (const choice of choices) {
    if (choice === value) return choice
  }
  return undefined
}
