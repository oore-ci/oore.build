import { useCallback, useLayoutEffect, useState } from 'react'

type KnownQuery = '(prefers-color-scheme: dark)'

type IMediaQuery = Array<KnownQuery | (string & {})>

type IMatchedMedia = boolean[]

function useMatchMedia(
  queries: IMediaQuery,
  defaultValues: IMatchedMedia = [],
): IMatchedMedia {
  const initialValues = defaultValues.length
    ? defaultValues
    : Array(queries.length).fill(false)

  if (typeof window === 'undefined') return initialValues

  const getMediaQueryLists = useCallback(
    () => queries.map((q) => window.matchMedia(q)),
    [queries],
  )
  const getValue = useCallback(
    () => getMediaQueryLists().map((mql) => mql.matches),
    [getMediaQueryLists],
  )

  // State and setter for matched value
  const [value, setValue] = useState(getValue)

  useLayoutEffect(() => {
    const mediaQueryLists = getMediaQueryLists()
    const handler = () => setValue(getValue)
    mediaQueryLists.forEach((mql) => mql.addEventListener('change', handler))
    return () =>
      mediaQueryLists.forEach((mql) =>
        mql.removeEventListener('change', handler),
      )
  }, [getMediaQueryLists, getValue])

  return value
}

export default useMatchMedia
