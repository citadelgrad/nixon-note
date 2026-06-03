import { useEffect, useRef, useState } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import rehypeHighlight from 'rehype-highlight'
import type { Note } from '../api'
import { CodeBlock } from './CodeBlock'
import 'highlight.js/styles/github.css'

interface NoteSidePanelProps {
  note: Note
  onClose: () => void
  searchQuery?: string
}

// Warm color palette for tags (matching NoteCard)
const TAG_COLORS = [
  'bg-sage-100 text-sage-700',
  'bg-amber-100 text-amber-700',
  'bg-orange-100 text-orange-700',
  'bg-rose-100 text-rose-700',
  'bg-emerald-100 text-emerald-700',
  'bg-teal-100 text-teal-700',
  'bg-lime-100 text-lime-700',
  'bg-yellow-100 text-yellow-700',
]

function tagColor(tagName: string): string {
  let hash = 0
  for (let i = 0; i < tagName.length; i++) {
    hash = (hash << 5) - hash + tagName.charCodeAt(i)
    hash = hash & hash
  }
  return TAG_COLORS[Math.abs(hash) % TAG_COLORS.length]
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

function normalizeMarkdown(content: string): string {
  return content
    .replace(/(\S)\n(#+\s)/g, '$1\n\n$2')
    .replace(/(\S)\n(---+)\n/g, '$1\n\n$2\n\n')
    .trim()
}

// Highlight search terms in content
function highlightSearchTerms(content: string, query: string): string {
  if (!query || query.trim() === '') return content

  const terms = query.trim().split(/\s+/).filter(t => t.length > 2)
  if (terms.length === 0) return content

  let highlighted = content
  terms.forEach(term => {
    const escapedTerm = term.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
    const regex = new RegExp(`(${escapedTerm})`, 'gi')
    highlighted = highlighted.replace(regex, '**$1**')
  })

  return highlighted
}

function TagsCollapsible({ tags }: { tags: string[] }) {
  const [expanded, setExpanded] = useState(false)

  return (
    <div className="mt-3">
      <button
        onClick={() => setExpanded(!expanded)}
        className="inline-flex items-center gap-1.5 text-xs text-sage-500 hover:text-sage-700 transition-colors"
      >
        <svg
          className={`w-3.5 h-3.5 transition-transform duration-200 ${expanded ? 'rotate-90' : ''}`}
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
        </svg>
        {tags.length} {tags.length === 1 ? 'tag' : 'tags'}
      </button>
      {expanded && (
        <div className="flex flex-wrap gap-1.5 mt-2">
          {tags.map((tag) => (
            <span
              key={tag}
              className={`px-2.5 py-1 rounded-full text-xs font-medium transition-colors ${tagColor(tag)}`}
            >
              {tag}
            </span>
          ))}
        </div>
      )}
    </div>
  )
}

export function NoteSidePanel({ note, onClose, searchQuery }: NoteSidePanelProps) {
  const panelRef = useRef<HTMLDivElement>(null)
  const content = normalizeMarkdown(note.content)
  const highlightedContent = searchQuery ? highlightSearchTerms(content, searchQuery) : content

  // Close on Escape key
  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', handleEscape)
    return () => window.removeEventListener('keydown', handleEscape)
  }, [onClose])

  // Prevent body scroll when panel is open
  useEffect(() => {
    document.body.style.overflow = 'hidden'
    return () => {
      document.body.style.overflow = ''
    }
  }, [])

  return (
    <div className="fixed inset-0 z-50 flex">
      {/* Backdrop with blur */}
      <div
        className="flex-1 bg-sage-900/20 backdrop-blur-sm transition-all duration-500"
        onClick={onClose}
        style={{ animation: 'fadeIn 0.4s ease-out' }}
      />

      {/* Side Panel */}
      <div
        ref={panelRef}
        className="w-full max-w-2xl bg-cream-50 shadow-2xl flex flex-col"
        style={{
          animation: 'slideInRight 0.5s cubic-bezier(0.22, 1, 0.36, 1)',
          borderLeft: '1px solid rgba(139, 147, 136, 0.15)',
          boxShadow: '-8px 0 40px rgba(139, 147, 136, 0.12), -2px 0 8px rgba(139, 147, 136, 0.08)',
        }}
      >
        {/* Header */}
        <div
          className="flex-shrink-0 px-8 py-6 border-b border-sage-100/60"
          style={{
            background: 'linear-gradient(to bottom, rgba(252, 251, 246, 1), rgba(252, 251, 246, 0.95))',
          }}
        >
          <div className="flex items-start justify-between gap-4">
            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-3 mb-2">
                <span className="text-sm text-sage-500 font-light">
                  {timeAgo(note.created_at)}
                </span>
                <span className="w-1 h-1 rounded-full bg-sage-300" />
                <span className="rounded-full bg-sage-100 px-2.5 py-0.5 text-xs text-sage-600 font-medium">
                  {note.source_type}
                </span>
              </div>
              {note.title && (
                <h2 className="text-xl font-medium text-sage-800 leading-relaxed">
                  {note.title}
                </h2>
              )}
            </div>

            {/* Close Button */}
            <button
              onClick={onClose}
              className="flex-shrink-0 w-10 h-10 rounded-full bg-sage-100/60 hover:bg-sage-200 text-sage-600 hover:text-sage-800 transition-all duration-200 flex items-center justify-center group"
              aria-label="Close panel"
              style={{ backdropFilter: 'blur(8px)' }}
            >
              <svg
                className="w-5 h-5 transition-transform duration-200 group-hover:rotate-90"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
              >
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          {/* Tags - collapsed by default, click to expand */}
          {note.tags && note.tags.length > 0 && (
            <TagsCollapsible tags={note.tags} />
          )}
        </div>

        {/* Content */}
        <div
          className="flex-1 overflow-y-auto px-8 py-8"
          style={{
            background: 'linear-gradient(to bottom, rgba(252, 251, 246, 0.95), rgba(252, 251, 246, 1))',
          }}
        >
          {note.summary && (
            <div
              className="mb-6 p-4 rounded-xl border border-sage-100/60"
              style={{
                background: 'linear-gradient(135deg, rgba(234, 238, 231, 0.3), rgba(234, 238, 231, 0.5))',
                backdropFilter: 'blur(8px)',
              }}
            >
              <p className="text-xs font-semibold text-sage-600 mb-2 uppercase tracking-widest">
                Summary
              </p>
              <p className="text-sage-700 leading-relaxed">{note.summary}</p>
            </div>
          )}

          <div
            className="prose prose-sage prose-sm max-w-none text-sage-800"
            style={{
              // Enhanced typography for readability
              fontSize: '15px',
              lineHeight: '1.75',
            }}
          >
            <ReactMarkdown
              remarkPlugins={[remarkGfm]}
              rehypePlugins={[rehypeHighlight]}
              components={{
                code: ({ className, children, ...props }) => {
                  if (!className) {
                    return <code {...props}>{children}</code>
                  }
                  return <CodeBlock className={className} {...props}>{String(children)}</CodeBlock>
                },
                // Highlight search terms in markdown
                p: ({ children }) => <p className="search-highlight-container">{children}</p>,
                strong: ({ children }) => {
                  // Check if this is a search highlight
                  const text = String(children)
                  if (searchQuery && searchQuery.split(/\s+/).some(term =>
                    text.toLowerCase().includes(term.toLowerCase())
                  )) {
                    return (
                      <strong
                        className="bg-amber-200/60 text-amber-900 px-1 rounded"
                        style={{ fontWeight: 600 }}
                      >
                        {children}
                      </strong>
                    )
                  }
                  return <strong>{children}</strong>
                },
              }}
            >
              {highlightedContent}
            </ReactMarkdown>
          </div>
        </div>
      </div>

      {/* CSS Animations */}
      <style>{`
        @keyframes fadeIn {
          from {
            opacity: 0;
          }
          to {
            opacity: 1;
          }
        }

        @keyframes slideInRight {
          from {
            transform: translateX(100%);
            opacity: 0;
          }
          to {
            transform: translateX(0);
            opacity: 1;
          }
        }

        /* Smooth scroll behavior */
        .overflow-y-auto {
          scroll-behavior: smooth;
        }

        /* Custom scrollbar for side panel */
        .overflow-y-auto::-webkit-scrollbar {
          width: 8px;
        }

        .overflow-y-auto::-webkit-scrollbar-track {
          background: rgba(234, 238, 231, 0.3);
          border-radius: 8px;
        }

        .overflow-y-auto::-webkit-scrollbar-thumb {
          background: rgba(139, 147, 136, 0.3);
          border-radius: 8px;
          transition: background 0.2s;
        }

        .overflow-y-auto::-webkit-scrollbar-thumb:hover {
          background: rgba(139, 147, 136, 0.5);
        }
      `}</style>
    </div>
  )
}
