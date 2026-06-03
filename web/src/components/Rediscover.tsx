import { useCallback, useEffect, useState } from 'react'
import { fetchRandomNotes, type Note } from '../api'
import { NoteCard } from './NoteCard'

interface RediscoverProps {
  onTagClick?: (tag: string) => void
  onNoteExpand?: (note: Note) => void
}

export function Rediscover({ onTagClick, onNoteExpand }: RediscoverProps) {
  const [notes, setNotes] = useState<Note[]>([])
  const [loading, setLoading] = useState(true)

  const refresh = useCallback(async () => {
    setLoading(true)
    try {
      const res = await fetchRandomNotes(3, 'hidden')
      setNotes(res.notes)
    } catch {
      // Silently fail — widget is non-critical
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    refresh()
  }, [refresh])

  if (loading) {
    return (
      <div className="rounded-xl border border-sage-100 bg-sage-50/30 p-5">
        <div className="flex items-center gap-2 mb-4">
          <div className="h-3 w-24 bg-sage-100 rounded animate-pulse" />
        </div>
        <div className="space-y-3">
          <div className="h-16 bg-sage-100/50 rounded-lg animate-pulse" />
          <div className="h-16 bg-sage-100/50 rounded-lg animate-pulse" />
        </div>
      </div>
    )
  }

  if (notes.length === 0) return null

  return (
    <div className="rounded-xl border border-sage-100 bg-sage-50/30 p-5">
      <div className="flex items-center justify-between mb-4">
        <h3 className="text-xs font-semibold text-sage-400 uppercase tracking-widest">
          Rediscover
        </h3>
        <button
          onClick={refresh}
          className="text-xs text-sage-400 hover:text-sage-600 transition-colors flex items-center gap-1"
          title="Show different notes"
        >
          <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0 3.181 3.183a8.25 8.25 0 0 0 13.803-3.7M4.031 9.865a8.25 8.25 0 0 1 13.803-3.7l3.181 3.182" />
          </svg>
          Shuffle
        </button>
      </div>
      <div className="space-y-3">
        {notes.map((note) => (
          <NoteCard
            key={note.id}
            note={note}
            onTagClick={onTagClick}
            onExpand={onNoteExpand}
          />
        ))}
      </div>
    </div>
  )
}
