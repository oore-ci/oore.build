import { useEffect, useState } from 'react'

function useObjectUrl(input?: Blob | MediaSource) {
  const [url, setUrl] = useState<string | undefined>()

  useEffect(() => {
    const nextUrl = input ? URL.createObjectURL(input) : undefined

    // The URL is a browser resource created after commit and revoked on cleanup.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setUrl(nextUrl)

    return () => {
      if (!nextUrl) return
      return URL.revokeObjectURL(nextUrl)
    }
  }, [input])

  return url
}

export { useObjectUrl }
