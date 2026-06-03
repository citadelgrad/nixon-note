import { useCallback, useEffect, useRef, useState } from 'react'

function isUrl(text: string): boolean {
  return /^https?:\/\/\S+$/i.test(text.trim())
}

function isYouTubeUrl(text: string): boolean {
  const t = text.trim()
  return /^https?:\/\/(www\.)?(youtube\.com\/watch|youtu\.be\/|youtube\.com\/embed\/|m\.youtube\.com\/watch)/i.test(t)
}

const DRAFT_KEY = 'nixonnote:capture-draft'

function saveDraft(text: string) {
  if (text.trim()) {
    localStorage.setItem(DRAFT_KEY, text)
  } else {
    localStorage.removeItem(DRAFT_KEY)
  }
}

function loadDraft(): string {
  return localStorage.getItem(DRAFT_KEY) ?? ''
}

function clearDraft() {
  localStorage.removeItem(DRAFT_KEY)
}

interface CaptureInputProps {
  onCapture: (content: string) => void
  onIngestUrl?: (url: string) => void
  disabled?: boolean
  ingesting?: boolean
  inline?: boolean // Show inline capture (for search view) vs modal-only (for chat view)
}

export function CaptureInput({ onCapture, onIngestUrl, disabled, ingesting, inline = false }: CaptureInputProps) {
  const [value, setValue] = useState('')
  const [isModalOpen, setIsModalOpen] = useState(false)
  const [isExpanded, setIsExpanded] = useState(false)
  const [hasDraft, setHasDraft] = useState(() => !!loadDraft())
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const inlineTextareaRef = useRef<HTMLTextAreaElement>(null)

  const handleChange = useCallback((text: string) => {
    setValue(text)
    saveDraft(text)
  }, [])

  const trimmed = value.trim()
  const showClipButton = isUrl(trimmed) && !!onIngestUrl

  useEffect(() => {
    if (isModalOpen) {
      queueMicrotask(() => {
        const draft = loadDraft()
        if (draft) setValue(draft)
        setHasDraft(false)
        textareaRef.current?.focus()
      })
    }
  }, [isModalOpen])

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      // Cmd+K or Ctrl+K to open modal
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault()
        if (!isModalOpen) {
          const draft = loadDraft()
          if (draft) setValue(draft)
          setHasDraft(false)
        }
        setIsModalOpen(true)
      }
      // Escape to close modal
      if (e.key === 'Escape' && isModalOpen) {
        setIsModalOpen(false)
      }
      // Cmd+Enter or Ctrl+Enter to submit
      if ((e.metaKey || e.ctrlKey) && e.key === 'Enter' && isModalOpen && trimmed) {
        e.preventDefault()
        onCapture(trimmed)
        setValue('')
        clearDraft()
        setIsModalOpen(false)
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [isModalOpen, trimmed, onCapture])

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    if (!trimmed) return
    onCapture(trimmed)
    setValue('')
    clearDraft()
    setIsModalOpen(false)
    setIsExpanded(false)
  }

  function handleClipArticle() {
    if (!trimmed || !onIngestUrl) return
    onIngestUrl(trimmed)
    setValue('')
    clearDraft()
    setIsModalOpen(false)
    setIsExpanded(false)
  }

  function handleExpand() {
    const draft = loadDraft()
    if (draft) setValue(draft)
    setIsExpanded(true)
    setTimeout(() => inlineTextareaRef.current?.focus(), 100)
  }

  const isYoutube = isYouTubeUrl(trimmed)

  const clipButton = (size: 'sm' | 'lg') => {
    if (!showClipButton) return null
    const baseClasses = size === 'lg'
      ? 'rounded-full px-8 py-3 font-medium text-sm'
      : 'rounded-full px-6 py-2 text-sm font-medium'
    const colorClasses = isYoutube
      ? 'bg-rose-600 text-white hover:bg-rose-700 focus:ring-rose-400/40'
      : 'bg-amber-500 text-white hover:bg-amber-600 focus:ring-amber-400/40'
    return (
      <button
        type="button"
        onClick={handleClipArticle}
        disabled={disabled || ingesting}
        className={`${baseClasses} ${colorClasses}
                   focus:outline-none focus:ring-2
                   transition-all disabled:opacity-40 disabled:cursor-not-allowed shadow-sm`}
      >
        {ingesting ? (
          <span className="flex items-center gap-2">
            <svg className="w-4 h-4 animate-spin" fill="none" viewBox="0 0 24 24">
              <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
              <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
            </svg>
            {isYoutube ? 'Importing...' : 'Extracting...'}
          </span>
        ) : (
          isYoutube ? 'Import YouTube' : 'Clip Article'
        )}
      </button>
    )
  }

  // Modal for expanded capture
  if (isModalOpen) {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-sage-900/50 backdrop-blur-sm p-4"
        onClick={(e) => e.target === e.currentTarget && !value.trim() && setIsModalOpen(false)}
      >
        <div className="w-full max-w-5xl h-[80vh] bg-cream-50 rounded-3xl shadow-2xl flex flex-col overflow-hidden border border-sage-100/60"
          style={{
            backgroundImage: `
              radial-gradient(circle at 10% 20%, rgba(139, 154, 120, 0.02) 0%, transparent 50%),
              radial-gradient(circle at 90% 80%, rgba(107, 125, 90, 0.03) 0%, transparent 50%)
            `
          }}
        >
          <div className="flex items-center justify-between px-8 py-6 border-b border-sage-100/60 bg-white/50 backdrop-blur-sm">
            <div>
              <h2 className="text-2xl font-light tracking-tight text-sage-800">Capture Thought</h2>
              <p className="text-sm text-sage-400 font-light mt-1">
                {value && !hasDraft ? 'Draft auto-saved' : 'Markdown supported'}
              </p>
            </div>
            <button
              type="button"
              onClick={() => setIsModalOpen(false)}
              className="w-10 h-10 flex items-center justify-center rounded-full text-sage-400 hover:text-sage-700 hover:bg-sage-100 transition-all"
            >
              <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          <form onSubmit={handleSubmit} className="flex-1 flex flex-col min-h-0">
            <textarea
              ref={textareaRef}
              value={value}
              onChange={(e) => handleChange(e.target.value)}
              placeholder="Capture an idea, task, or note..."
              disabled={disabled || ingesting}
              className="flex-1 px-8 py-6 border-none outline-none resize-none text-sage-800 placeholder:text-sage-300 text-lg leading-relaxed font-light bg-transparent"
            />

            <div className="px-8 py-6 border-t border-sage-100/60 flex justify-between items-center bg-white/50 backdrop-blur-sm">
              <div className="text-xs text-sage-400 font-light flex items-center gap-3">
                <span className="flex items-center gap-1.5">
                  <kbd className="px-2 py-1 bg-sage-50 border border-sage-200/60 rounded text-xs">⌘K</kbd>
                  <span>to open</span>
                </span>
                <span className="text-sage-200">•</span>
                <span className="flex items-center gap-1.5">
                  <kbd className="px-2 py-1 bg-sage-50 border border-sage-200/60 rounded text-xs">Esc</kbd>
                  <span>to close</span>
                </span>
                <span className="text-sage-200">•</span>
                <span className="flex items-center gap-1.5">
                  <kbd className="px-2 py-1 bg-sage-50 border border-sage-200/60 rounded text-xs">⌘↵</kbd>
                  <span>to save</span>
                </span>
              </div>
              <div className="flex items-center gap-3">
                {clipButton('lg')}
                <button
                  type="submit"
                  disabled={disabled || ingesting || !trimmed}
                  className="rounded-full bg-sage-500 px-8 py-3 font-medium text-cream
                             hover:bg-sage-600 focus:outline-none focus:ring-2 focus:ring-sage-400/40
                             transition-all disabled:opacity-40 disabled:cursor-not-allowed shadow-sm"
                >
                  Save Note
                </button>
              </div>
            </div>
          </form>
        </div>
      </div>
    )
  }

  // Inline capture for search view
  if (inline) {
    return (
      <div className="mb-8">
        {!isExpanded ? (
          <button
            onClick={handleExpand}
            disabled={disabled}
            className="w-full group"
          >
            <div className="relative overflow-hidden rounded-2xl border border-sage-200/60 bg-white/80 backdrop-blur-sm px-6 py-4
                          hover:border-sage-400/60 hover:bg-white transition-all shadow-sm">
              <div className="flex items-center justify-between">
                <span className="text-sage-400 font-light flex items-center gap-3">
                  <svg className="w-5 h-5 text-sage-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
                  </svg>
                  <span>{loadDraft() ? 'Resume draft...' : 'Quick capture...'}</span>
                </span>
                <span className="text-xs text-sage-300 font-light flex items-center gap-1.5">
                  <kbd className="px-2 py-0.5 bg-sage-50 border border-sage-200/60 rounded text-xs">⌘K</kbd>
                  <span>for expanded</span>
                </span>
              </div>
            </div>
          </button>
        ) : (
          <form onSubmit={handleSubmit} className="rounded-2xl border border-sage-200/60 bg-white shadow-sm overflow-hidden">
            <textarea
              ref={inlineTextareaRef}
              value={value}
              onChange={(e) => handleChange(e.target.value)}
              placeholder="Capture anything - ideas, tasks, links, notes... (⏎ to save, Esc to cancel)"
              disabled={disabled || ingesting}
              onKeyDown={(e) => {
                if (e.key === 'Escape') {
                  setIsExpanded(false)
                }
                if (e.key === 'Enter' && !e.shiftKey) {
                  e.preventDefault()
                  handleSubmit(e)
                }
              }}
              className="w-full px-6 py-4 border-none outline-none resize-none text-sage-800 placeholder:text-sage-300 font-light leading-relaxed"
              rows={3}
            />
            <div className="px-6 py-3 border-t border-sage-100/60 flex justify-between items-center bg-sage-50/30">
              <button
                type="button"
                onClick={() => setIsExpanded(false)}
                className="text-sm text-sage-400 hover:text-sage-600 font-medium transition-colors"
              >
                Cancel
              </button>
              <div className="flex items-center gap-3">
                {clipButton('sm')}
                <button
                  type="submit"
                  disabled={disabled || ingesting || !trimmed}
                  className="rounded-full bg-sage-500 px-6 py-2 text-sm font-medium text-cream
                             hover:bg-sage-600 transition-all disabled:opacity-40 disabled:cursor-not-allowed shadow-sm"
                >
                  Save
                </button>
              </div>
            </div>
          </form>
        )}
      </div>
    )
  }

  // No UI in chat view - modal only via ⌘K
  return null
}
