import { useCallback, useEffect, useState } from 'react'
import { getToken, setToken, onAuthError } from '../api'

export function TokenGate({ children }: { children: React.ReactNode }) {
  const [authenticated, setAuthenticated] = useState(() => !!getToken())
  const [input, setInput] = useState('')
  const [testing, setTesting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const handleLogout = useCallback(() => {
    setToken('')
    setAuthenticated(false)
  }, [])

  useEffect(() => {
    onAuthError(handleLogout)
    return () => onAuthError(null)
  }, [handleLogout])

  async function handleConnect(e: React.FormEvent) {
    e.preventDefault()
    const value = input.trim()
    if (!value) return

    setTesting(true)
    setError(null)

    try {
      const res = await fetch('/api/tags', {
        headers: { Authorization: `Bearer ${value}` },
      })
      if (res.ok) {
        setToken(value)
        setAuthenticated(true)
      } else if (res.status === 401) {
        setError('Invalid token.')
      } else {
        setError(`Server returned ${res.status}.`)
      }
    } catch {
      setError('Cannot reach server.')
    } finally {
      setTesting(false)
    }
  }

  if (authenticated) return <>{children}</>

  return (
    <div className="min-h-screen flex items-center justify-center bg-cream-50">
      <form onSubmit={handleConnect} className="w-full max-w-sm px-6">
        <div className="text-center mb-8">
          <h1 className="text-3xl font-light tracking-tight text-sage-800 mb-1">Note</h1>
          <p className="text-sm text-sage-400 font-light">Enter your token to connect</p>
        </div>

        <input
          type="password"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="API token"
          autoFocus
          className="w-full px-4 py-3 rounded-xl border border-sage-200 bg-white text-sage-800 placeholder:text-sage-300 focus:outline-none focus:ring-2 focus:ring-sage-300 focus:border-transparent"
        />

        {error && (
          <p className="mt-3 text-sm text-muted-red text-center">{error}</p>
        )}

        <button
          type="submit"
          disabled={testing || !input.trim()}
          className="mt-4 w-full py-3 rounded-xl bg-sage-600 text-cream font-medium hover:bg-sage-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {testing ? 'Connecting...' : 'Connect'}
        </button>
      </form>
    </div>
  )
}
