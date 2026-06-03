import { useEffect, useState } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import rehypeHighlight from 'rehype-highlight'
import { fetchNote } from '../api'
import type { Note } from '../api'
import 'highlight.js/styles/github.css'

interface NoteModalProps {
  noteId: number
  onClose: () => void
}

function timeAgo(dateStr: string): string {
  const date = new Date(dateStr + 'Z')
  const now = new Date()
  const seconds = Math.floor((now.getTime() - date.getTime()) / 1000)

  if (seconds < 60) return 'just now'
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`
  if (seconds < 604800) return `${Math.floor(seconds / 86400)}d ago`
  return date.toLocaleDateString()
}

export function NoteModal({ noteId, onClose }: NoteModalProps) {
  const [note, setNote] = useState<Note | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    async function loadNote() {
      try {
        setLoading(true)
        const data = await fetchNote(noteId)
        setNote(data)
      } catch (err) {
        const message = err instanceof Error ? err.message : 'Failed to load note'
        console.error('Failed to load note:', err)
        setError(message)
      } finally {
        setLoading(false)
      }
    }

    loadNote()
  }, [noteId])

  // Close on escape key
  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', handleEscape)
    return () => window.removeEventListener('keydown', handleEscape)
  }, [onClose])

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm p-4"
      onClick={onClose}
    >
      <div
        className="bg-white rounded-2xl shadow-2xl max-w-3xl w-full max-h-[85vh] overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-sage-100">
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 rounded-full bg-sage-100 flex items-center justify-center">
              <svg className="w-5 h-5 text-sage-600" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
              </svg>
            </div>
            <div>
              <h2 className="text-lg font-semibold text-sage-800">Note #{noteId}</h2>
              {note && (
                <p className="text-sm text-sage-500">
                  {timeAgo(note.created_at)} • {note.source_type}
                </p>
              )}
            </div>
          </div>
          <button
            onClick={onClose}
            className="text-sage-400 hover:text-sage-600 transition-colors"
            aria-label="Close modal"
          >
            <svg className="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* Content */}
        <div className="overflow-y-auto px-6 py-6" style={{ maxHeight: 'calc(85vh - 80px)' }}>
          {loading && (
            <div className="flex items-center justify-center py-12">
              <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-sage-500"></div>
            </div>
          )}

          {error && (
            <div className="bg-muted-red/5 border border-muted-red/20 rounded-lg p-4 text-center">
              <p className="text-muted-red font-medium mb-2">Failed to load note</p>
              <p className="text-sage-600 text-sm">{error}</p>
            </div>
          )}

          {note && !loading && !error && (
            <div className="space-y-4">
              {note.title && (
                <h3 className="text-xl font-semibold text-sage-800">{note.title}</h3>
              )}

              {note.summary && (
                <div className="bg-sage-50/50 border border-sage-100 rounded-lg p-4">
                  <p className="text-sm font-medium text-sage-600 mb-1">Summary</p>
                  <p className="text-sage-700">{note.summary}</p>
                </div>
              )}

              <div className="prose prose-sage prose-sm max-w-none text-sage-800">
                <ReactMarkdown
                  remarkPlugins={[remarkGfm]}
                  rehypePlugins={[rehypeHighlight]}
                >
                  {note.content}
                </ReactMarkdown>
              </div>

              {note.tags && note.tags.length > 0 && (
                <div className="flex flex-wrap gap-2 pt-2">
                  {note.tags.map((tag) => (
                    <span
                      key={tag}
                      className="px-3 py-1 rounded-full text-xs font-medium bg-sage-100 text-sage-700"
                    >
                      {tag}
                    </span>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
