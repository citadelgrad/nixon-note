import { useEffect, useState } from 'react'
import { fetchNotes } from '../api'

const GREETINGS = [
  'Your notes are here for you',
  'Ready when you are',
  'Welcome back to your thoughts',
  'Your knowledge base awaits',
  'Pick up where you left off',
]

export function WelcomeBack({ onDismiss }: { onDismiss: () => void }) {
  const [recentCount, setRecentCount] = useState<number | null>(null)
  const [greeting] = useState(() => GREETINGS[Math.floor(Math.random() * GREETINGS.length)])

  useEffect(() => {
    fetchNotes(undefined, 1, 0, 'hidden')
      .then(res => setRecentCount(res.count))
      .catch(() => {})
  }, [])

  return (
    <div className="rounded-2xl border border-sage-100 bg-gradient-to-br from-sage-50/80 to-cream-50 p-8 mb-8 relative">
      <button
        onClick={onDismiss}
        className="absolute top-4 right-4 text-sage-300 hover:text-sage-500 transition-colors"
        title="Dismiss"
      >
        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>

      <div className="flex items-start gap-5">
        <div className="w-12 h-12 rounded-full bg-sage-100 flex items-center justify-center flex-shrink-0">
          <svg className="w-6 h-6 text-sage-500" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M12 6.042A8.967 8.967 0 0 0 6 3.75c-1.052 0-2.062.18-3 .512v14.25A8.987 8.987 0 0 1 6 18c2.305 0 4.408.867 6 2.292m0-14.25a8.966 8.966 0 0 1 6-2.292c1.052 0 2.062.18 3 .512v14.25A8.987 8.987 0 0 0 18 18a8.967 8.967 0 0 0-6 2.292m0-14.25v14.25" />
          </svg>
        </div>

        <div>
          <h2 className="text-lg font-light text-sage-700 mb-1">{greeting}</h2>
          {recentCount !== null && recentCount > 0 && (
            <p className="text-sm text-sage-400 font-light">
              You have {recentCount} {recentCount === 1 ? 'note' : 'notes'} in your collection.
            </p>
          )}
        </div>
      </div>
    </div>
  )
}
