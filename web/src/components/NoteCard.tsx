import { useState, useRef, useEffect, useCallback } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import rehypeHighlight from 'rehype-highlight'
import type { Note } from '../api'
import { deleteNote, updateNote, generateAudio } from '../api'
import { validateMarkdown, type MarkdownValidationIssue } from '../utils/markdownValidator'
import { CodeBlock } from './CodeBlock'
import 'highlight.js/styles/github.css'

interface NoteCardProps {
  note: Note
  onDelete?: () => void
  onTagClick?: (tag: string) => void
  searchQuery?: string
  onExpand?: (note: Note) => void
  onAudioGenerated?: (episodeId: number) => void
}

// Warm color palette (no blue, ADHD-friendly)
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

// Hash tag name to consistent color index
function tagColor(tagName: string): string {
  let hash = 0
  for (let i = 0; i < tagName.length; i++) {
    hash = (hash << 5) - hash + tagName.charCodeAt(i)
    hash = hash & hash // Convert to 32-bit integer
  }
  return TAG_COLORS[Math.abs(hash) % TAG_COLORS.length]
}

// Get icon for special tags
function tagIcon(tagName: string): string | null {
  switch (tagName) {
    case 'hidden':
      return '👁️‍🗨️'
    case 'tool':
      return '🔧'
    case 'bookmark':
      return '🔖'
    default:
      return null
  }
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
  // Minimal normalization - only fix truly broken markdown
  // ReactMarkdown handles most cases correctly without intervention
  return content
    // Ensure headers have blank line before them (only if preceded by text)
    .replace(/(\S)\n(#+\s)/g, '$1\n\n$2')
    // Ensure horizontal rules have blank lines around them
    .replace(/(\S)\n(---+)\n/g, '$1\n\n$2\n\n')
    .trim()
}

function NoteCardTags({ tags, onTagClick }: { tags: string[], onTagClick?: (tag: string) => void }) {
  const [expanded, setExpanded] = useState(false)

  return (
    <div className="mt-3">
      <button
        onClick={() => setExpanded(!expanded)}
        className="inline-flex items-center gap-1.5 text-xs text-sage-400 hover:text-sage-600 transition-colors"
      >
        <svg
          className={`w-3 h-3 transition-transform duration-200 ${expanded ? 'rotate-90' : ''}`}
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
        </svg>
        {tags.length} {tags.length === 1 ? 'tag' : 'tags'}
      </button>
      {expanded && (
        <div className="flex flex-wrap gap-1.5 mt-1.5">
          {tags.map((tag) => {
            const icon = tagIcon(tag)
            return (
              <button
                key={tag}
                onClick={() => onTagClick?.(tag)}
                className={`px-2 py-0.5 rounded-full text-xs font-medium transition-colors ${tagColor(tag)} hover:opacity-80 inline-flex items-center gap-1`}
                title={`Filter notes by ${tag}`}
              >
                {icon && <span className="text-xs">{icon}</span>}
                {tag}
              </button>
            )
          })}
        </div>
      )}
    </div>
  )
}

export function NoteCard({ note, onDelete, onTagClick, searchQuery, onExpand, onAudioGenerated }: NoteCardProps) {
  const [isDeleting, setIsDeleting] = useState(false)
  const [isGeneratingAudio, setIsGeneratingAudio] = useState(false)
  const [audioToast, setAudioToast] = useState<string | null>(null)
  const [isEditing, setIsEditing] = useState(false)
  const [editContent, setEditContent] = useState(note.content)
  const [isSaving, setIsSaving] = useState(false)
  const [activeTab, setActiveTab] = useState<'edit' | 'preview'>('edit')
  const [validationIssues, setValidationIssues] = useState<MarkdownValidationIssue[]>([])
  const [isTruncated, setIsTruncated] = useState(false)
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const contentRef = useRef<HTMLDivElement>(null)
  const content = normalizeMarkdown(note.content)

  useEffect(() => {
    if (isEditing && textareaRef.current) {
      textareaRef.current.focus()
      // Auto-resize textarea
      textareaRef.current.style.height = 'auto'
      textareaRef.current.style.height = textareaRef.current.scrollHeight + 'px'
    }
  }, [isEditing])

  // Check if content should be truncated based on rendered height
  useEffect(() => {
    if (isEditing) {
      setIsTruncated(false)
      return
    }

    // Use a slight delay to ensure markdown has fully rendered
    const timer = setTimeout(() => {
      if (contentRef.current) {
        const height = contentRef.current.scrollHeight
        setIsTruncated(height > 400)
      }
    }, 50)

    return () => clearTimeout(timer)
  }, [note.content, note.id, isEditing])

  // Validate markdown when editing
  useEffect(() => {
    if (isEditing) {
      const issues = validateMarkdown(editContent)
      setValidationIssues(issues)
    }
  }, [editContent, isEditing])

  async function handleDelete() {
    if (!confirm('Delete this note? This cannot be undone.')) {
      return
    }

    setIsDeleting(true)
    try {
      await deleteNote(note.id)
      onDelete?.()
    } catch (err) {
      console.error('Failed to delete note:', err)
      alert('Failed to delete note. Please try again.')
      setIsDeleting(false)
    }
  }

  const handleSave = useCallback(async () => {
    if (editContent.trim() === '') {
      alert('Content cannot be empty')
      return
    }

    setIsSaving(true)
    try {
      await updateNote(note.id, editContent)
      setIsEditing(false)
      onDelete?.() // Trigger refresh to show updated content
    } catch (err) {
      console.error('Failed to update note:', err)
      alert('Failed to update note. Please try again.')
    } finally {
      setIsSaving(false)
    }
  }, [editContent, note.id, onDelete])

  const handleCancel = useCallback(() => {
    setEditContent(note.content)
    setIsEditing(false)
  }, [note.content])

  // Keyboard shortcuts for editing
  useEffect(() => {
    if (!isEditing) return

    const handleKeyDown = (e: KeyboardEvent) => {
      // Escape to cancel
      if (e.key === 'Escape') {
        e.preventDefault()
        handleCancel()
      }
      // Cmd+Enter (Mac) or Ctrl+Enter (Win/Linux) to save
      else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault()
        handleSave()
      }
      // Ctrl+Tab or Cmd+Tab to switch between Edit/Preview
      else if (e.key === 'Tab' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault()
        setActiveTab(activeTab === 'edit' ? 'preview' : 'edit')
      }
    }

    document.addEventListener('keydown', handleKeyDown)
    return () => document.removeEventListener('keydown', handleKeyDown)
  }, [activeTab, handleCancel, handleSave, isEditing])

  async function handleGenerateAudio() {
    setIsGeneratingAudio(true)
    try {
      const res = await generateAudio({ note_ids: [note.id] })
      onAudioGenerated?.(res.episode_id)
      setAudioToast('Audio generating — this may take a minute')
      setTimeout(() => setAudioToast(null), 5000)
    } catch (err) {
      console.error('Failed to generate audio:', err)
      const msg = err instanceof Error ? err.message : 'Failed to start audio generation'
      setAudioToast(msg)
      setTimeout(() => setAudioToast(null), 6000)
    } finally {
      setIsGeneratingAudio(false)
    }
  }

  const sourceBorder =
    note.source_type === 'homebrew' ? 'border-l-[3px] border-l-teal-200' :
    note.source_type === 'bookmark' ? 'border-l-[3px] border-l-amber-200' :
    note.source_type === 'tweet' ? 'border-l-[3px] border-l-sage-300' : ''

  return (
    <div className={`rounded-xl border border-sage-100 bg-white p-5 transition-shadow hover:shadow-md group relative ${sourceBorder}`}>
      {audioToast && (
        <div className={`absolute top-2 left-1/2 -translate-x-1/2 z-10 px-3 py-1.5 rounded-lg text-white text-xs shadow-lg max-w-[90%] text-center ${
          audioToast.includes('generating') ? 'bg-sage-700' : 'bg-muted-red'
        }`}>
          {audioToast}
        </div>
      )}
      {isEditing ? (
        <div className="space-y-3">
          {/* Tab Navigation */}
          <div className="flex gap-1 border-b border-sage-100">
            <button
              onClick={() => setActiveTab('edit')}
              className={`px-4 py-2 text-sm font-medium transition-colors ${
                activeTab === 'edit'
                  ? 'text-sage-700 border-b-2 border-sage-500'
                  : 'text-sage-400 hover:text-sage-600'
              }`}
            >
              Edit
            </button>
            <button
              onClick={() => setActiveTab('preview')}
              className={`px-4 py-2 text-sm font-medium transition-colors ${
                activeTab === 'preview'
                  ? 'text-sage-700 border-b-2 border-sage-500'
                  : 'text-sage-400 hover:text-sage-600'
              }`}
            >
              Preview
            </button>
          </div>

          {/* Tab Content */}
          {activeTab === 'edit' ? (
            <div className="space-y-2">
              <textarea
                ref={textareaRef}
                value={editContent}
                onChange={(e) => {
                  setEditContent(e.target.value)
                  // Auto-resize
                  e.target.style.height = 'auto'
                  e.target.style.height = e.target.scrollHeight + 'px'
                }}
                className="w-full min-h-[200px] p-3 border border-sage-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-sage-400 focus:border-transparent resize-none font-mono text-sm"
                placeholder="Edit note content..."
              />
              {/* Validation Issues */}
              {validationIssues.length > 0 && (
                <div className="space-y-1">
                  {validationIssues.slice(0, 5).map((issue, idx) => (
                    <div
                      key={idx}
                      className={`flex items-start gap-2 px-3 py-2 rounded-md text-xs ${
                        issue.severity === 'error'
                          ? 'bg-rose-50 text-rose-700 border border-rose-200'
                          : 'bg-amber-50 text-amber-700 border border-amber-200'
                      }`}
                    >
                      <span className="font-medium">
                        {issue.severity === 'error' ? '⚠️' : 'ℹ️'}
                      </span>
                      <div className="flex-1">
                        <div className="font-medium">Line {issue.line}:</div>
                        <div>{issue.message}</div>
                      </div>
                    </div>
                  ))}
                  {validationIssues.length > 5 && (
                    <div className="text-xs text-sage-500 px-3">
                      + {validationIssues.length - 5} more issue{validationIssues.length - 5 !== 1 ? 's' : ''}
                    </div>
                  )}
                </div>
              )}
            </div>
          ) : (
            <div className="min-h-[200px] p-3 border border-sage-200 rounded-lg bg-sage-50/30">
              <div className="prose prose-sage prose-sm max-w-none text-sage-800">
                <ReactMarkdown
                  remarkPlugins={[remarkGfm]}
                  rehypePlugins={[rehypeHighlight]}
                  components={{
                    code: ({ className, children, ...props }) => {
                      if (!className) {
                        return <code {...props}>{children}</code>
                      }
                      return <CodeBlock className={className} {...props}>{String(children)}</CodeBlock>
                    }
                  }}
                >
                  {normalizeMarkdown(editContent)}
                </ReactMarkdown>
              </div>
            </div>
          )}

          {/* Actions */}
          <div className="flex gap-2 justify-between items-center">
            <div className="text-xs text-sage-400">
              <kbd className="px-1.5 py-0.5 bg-sage-50 border border-sage-200 rounded text-sage-600">Esc</kbd> cancel ·
              <kbd className="px-1.5 py-0.5 bg-sage-50 border border-sage-200 rounded text-sage-600 ml-1">⌘⏎</kbd> save
            </div>
            <div className="flex gap-2">
              <button
                onClick={handleCancel}
                disabled={isSaving}
                className="px-4 py-2 text-sm font-medium text-sage-600 hover:text-sage-800 disabled:opacity-50"
                title="Cancel editing (Esc)"
              >
                Cancel
              </button>
              <button
                onClick={handleSave}
                disabled={isSaving}
                className="px-4 py-2 text-sm font-medium bg-sage-600 text-white rounded-lg hover:bg-sage-700 disabled:opacity-50"
                title="Save changes (⌘+Enter or Ctrl+Enter)"
              >
                {isSaving ? 'Saving...' : 'Save'}
              </button>
            </div>
          </div>
        </div>
      ) : (
        <div className="relative">
          <div
            ref={contentRef}
            className="prose prose-sage prose-sm max-w-none text-sage-800 transition-all duration-300"
            style={{
              maxHeight: isTruncated ? '400px' : 'none',
              overflow: isTruncated ? 'hidden' : 'visible',
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
                // Highlight search terms
                strong: ({ children }) => {
                  const text = String(children)
                  if (searchQuery && searchQuery.split(/\s+/).some(term =>
                    text.toLowerCase().includes(term.toLowerCase())
                  )) {
                    return (
                      <strong
                        className="bg-amber-200/60 text-amber-900 px-0.5 rounded"
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
              {content}
            </ReactMarkdown>
          </div>

          {/* Gradient Fade & Read More Button */}
          {isTruncated && (
            <div className="relative -mt-24 h-24 flex items-end justify-center">
              {/* Paper-like gradient fade */}
              <div
                className="absolute inset-0 pointer-events-none"
                style={{
                  background: 'linear-gradient(to bottom, rgba(255, 255, 255, 0) 0%, rgba(255, 255, 255, 0.6) 30%, rgba(255, 255, 255, 0.95) 70%, rgba(255, 255, 255, 1) 100%)',
                }}
              />

              {/* Read More Button */}
              <button
                onClick={() => onExpand?.(note)}
                className="relative z-10 group flex items-center gap-2 px-4 py-2 rounded-full bg-sage-100/80 hover:bg-sage-200/80 text-sage-700 hover:text-sage-900 text-sm font-medium transition-all duration-200 shadow-sm hover:shadow-md backdrop-blur-sm"
                style={{
                  border: '1px solid rgba(139, 147, 136, 0.2)',
                }}
              >
                <span>Read more</span>
                <svg
                  className="w-4 h-4 transition-transform duration-200 group-hover:translate-x-0.5"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                >
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                </svg>
              </button>
            </div>
          )}
        </div>
      )}
      {note.tags && note.tags.length > 0 && (
        <NoteCardTags tags={note.tags} onTagClick={onTagClick} />
      )}
      <div className="mt-3 flex items-center justify-between text-sm text-sage-400">
        <div className="flex items-center gap-3">
          <span>{timeAgo(note.created_at)}</span>
          <span className="rounded-full bg-sage-50 px-2 py-0.5 text-xs text-sage-500">
            {note.source_type}
          </span>
        </div>
        <div className="flex items-center gap-2 opacity-0 group-hover:opacity-100 transition-opacity">
          <button
            onClick={handleGenerateAudio}
            disabled={isGeneratingAudio}
            className="text-sage-400 hover:text-sage-600 disabled:opacity-50"
            title="Convert to audio"
          >
            {isGeneratingAudio ? (
              <svg className="w-4 h-4 animate-spin" fill="none" viewBox="0 0 24 24">
                <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
              </svg>
            ) : (
              <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M19.114 5.636a9 9 0 010 12.728M16.463 8.288a5.25 5.25 0 010 7.424M6.75 8.25l4.72-4.72a.75.75 0 011.28.53v15.88a.75.75 0 01-1.28.53l-4.72-4.72H4.51c-.88 0-1.704-.507-1.938-1.354A9.009 9.009 0 012.25 12c0-.83.112-1.633.322-2.396C2.806 8.756 3.63 8.25 4.51 8.25H6.75z" />
              </svg>
            )}
          </button>
          <button
            onClick={() => {
              setIsEditing(true)
              setActiveTab('edit')
            }}
            disabled={isEditing}
            className="text-sage-400 hover:text-sage-600 disabled:opacity-50"
            title="Edit note"
          >
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
            </svg>
          </button>
          <button
            onClick={handleDelete}
            disabled={isDeleting}
            className="text-sage-400 hover:text-muted-red disabled:opacity-50"
            title="Delete note"
          >
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
            </svg>
          </button>
        </div>
      </div>
    </div>
  )
}
