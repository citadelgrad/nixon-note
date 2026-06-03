import { useCallback, useEffect, useRef, useState } from 'react'
import { SearchBar } from './components/SearchBar'
import { NoteCard } from './components/NoteCard'
import { CaptureInput } from './components/CaptureInput'
import { TagFilter } from './components/TagFilter'
import { VoiceRecorder } from './components/VoiceRecorder'
import { DailyDigest } from './components/DailyDigest'
import { ChatInterface } from './components/ChatInterface'
import { NoteSkeleton } from './components/NoteSkeleton'
import { NoteSidePanel } from './components/NoteSidePanel'
import { Rediscover } from './components/Rediscover'
import { WelcomeBack } from './components/WelcomeBack'
import { Settings } from './components/Settings'
import { AudioPlayer } from './components/AudioPlayer'
import { TokenGate } from './components/TokenGate'
import { createNote, ingestUrl, ingestYoutube, fetchNote, fetchNotes, fetchNotesByTag, type Note } from './api'

type View = 'chat' | 'search' | 'settings'

type HashState = {
  view: View
  noteId: number | null
}

function parseHashState(): HashState {
  const raw = window.location.hash.replace(/^#\/?/, '')
  const [path, query = ''] = raw.split('?')
  const view: View = path === 'search' || path === 'settings' ? path : 'chat'
  const noteParam = new URLSearchParams(query).get('note')
  const parsed = noteParam ? Number.parseInt(noteParam, 10) : NaN

  return {
    view,
    noteId: Number.isFinite(parsed) && parsed > 0 ? parsed : null,
  }
}

export default function App() {
  useEffect(() => {
    const savedTheme = localStorage.getItem('note_theme')
    if (savedTheme) {
      document.documentElement.setAttribute('data-theme', savedTheme)
    }
  }, [])

  const [view, _setView] = useState<View>(() => parseHashState().view)
  const [deepLinkNoteId, setDeepLinkNoteId] = useState<number | null>(() => parseHashState().noteId)

  // Sync view → URL hash
  const setView = useCallback((v: View) => {
    _setView(v)
    window.location.hash = v === 'chat' ? '/' : `/${v}`
  }, [])

  // Listen for browser back/forward
  useEffect(() => {
    const onHashChange = () => {
      const next = parseHashState()
      _setView(next.view)
      setDeepLinkNoteId(next.noteId)
    }
    window.addEventListener('hashchange', onHashChange)
    return () => window.removeEventListener('hashchange', onHashChange)
  }, [])
  const [query, setQuery] = useState('')
  const [selectedTag, setSelectedTag] = useState<string | null>(null)
  const [notes, setNotes] = useState<Note[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const [hideImportedItems, setHideImportedItems] = useState(true)
  const [ingesting, setIngesting] = useState(false)
  const [expandedNote, setExpandedNote] = useState<Note | null>(null)
  const [showWelcome, setShowWelcome] = useState(() => !sessionStorage.getItem('welcome_dismissed'))
  const [playEpisodeId, setPlayEpisodeId] = useState<number | null>(null)
  const debounceRef = useRef<ReturnType<typeof setTimeout>>(null)

  useEffect(() => {
    if (!deepLinkNoteId) return

    const openDeepLinkedNote = async () => {
      try {
        const note = await fetchNote(deepLinkNoteId)
        setExpandedNote(note)
        _setView('search')
      } catch (e) {
        setError(e instanceof Error ? e.message : `Failed to open note #${deepLinkNoteId}`)
      }
    }

    void openDeepLinkedNote()
  }, [deepLinkNoteId])

  const loadNotes = useCallback(async (q: string, tag: string | null, hideImported: boolean) => {
    setLoading(true)
    setError(null)
    try {
      const res = tag
        ? await fetchNotesByTag(tag)
        : await fetchNotes(q || undefined, 20, 0, hideImported ? 'hidden' : undefined)
      setNotes(res.notes)
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load notes')
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current)
    debounceRef.current = setTimeout(() => loadNotes(query, selectedTag, hideImportedItems), query ? 200 : 0)
    return () => { if (debounceRef.current) clearTimeout(debounceRef.current) }
  }, [query, selectedTag, hideImportedItems, loadNotes])

  async function handleCapture(content: string) {
    setSaving(true)
    try {
      await createNote(content)
      await loadNotes(query, selectedTag, hideImportedItems)
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to save note')
    } finally {
      setSaving(false)
    }
  }

  async function handleIngestUrl(url: string) {
    setIngesting(true)
    setError(null)
    try {
      const isYoutube = /^https?:\/\/(www\.)?(youtube\.com\/watch|youtu\.be\/|youtube\.com\/embed\/|m\.youtube\.com\/watch)/i.test(url.trim())
      if (isYoutube) {
        await ingestYoutube(url)
      } else {
        await ingestUrl(url)
      }
      await loadNotes(query, selectedTag, hideImportedItems)
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to import URL')
    } finally {
      setIngesting(false)
    }
  }

  function handleSelectTag(tag: string | null) {
    setSelectedTag(tag)
    setQuery('') // Clear search when selecting tag
  }

  return (
    <TokenGate>
    <div className="min-h-screen flex flex-col">
      {/* Unified Header */}
      <header className="border-b border-sage-100 bg-cream-50/80 backdrop-blur-sm sticky top-0 z-50">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="flex items-end justify-between py-3">
            {/* Brand */}
            <div>
              <h1 className="text-2xl font-light tracking-tight text-sage-800 mb-0.5 leading-none">
                Note
              </h1>
              <p className="text-xs text-sage-400 font-light tracking-wide">
                Your externalized memory
              </p>
            </div>

            {/* View Toggle */}
            <nav className="flex gap-1 p-1 bg-sage-50 rounded-full border border-sage-100/60">
              <button
                onClick={() => setView('chat')}
                className={`px-5 py-2 rounded-full text-sm font-medium transition-all ${
                  view === 'chat'
                    ? 'bg-sage-500 text-cream shadow-sm'
                    : 'text-sage-600 hover:text-sage-700 hover:bg-sage-100/50'
                }`}
              >
                Chat
              </button>
              <button
                onClick={() => setView('search')}
                className={`px-5 py-2 rounded-full text-sm font-medium transition-all ${
                  view === 'search'
                    ? 'bg-sage-500 text-cream shadow-sm'
                    : 'text-sage-600 hover:text-sage-700 hover:bg-sage-100/50'
                }`}
              >
                Search
              </button>
            </nav>

            {/* Settings Gear Icon */}
            <button
              onClick={() => setView('settings')}
              className={`ml-3 p-2 rounded-full transition-all ${
                view === 'settings'
                  ? 'bg-sage-500 text-cream shadow-sm'
                  : 'text-sage-400 hover:text-sage-600 hover:bg-sage-100/50'
              }`}
              title="Settings"
            >
              <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.325.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 0 1 1.37.49l1.296 2.247a1.125 1.125 0 0 1-.26 1.431l-1.003.827c-.293.241-.438.613-.43.992a7.723 7.723 0 0 1 0 .255c-.008.378.137.75.43.991l1.004.827c.424.35.534.955.26 1.43l-1.298 2.247a1.125 1.125 0 0 1-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.47 6.47 0 0 1-.22.128c-.331.183-.581.495-.644.869l-.213 1.281c-.09.543-.56.94-1.11.94h-2.594c-.55 0-1.019-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 0 1-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 0 1-1.369-.49l-1.297-2.247a1.125 1.125 0 0 1 .26-1.431l1.004-.827c.292-.24.437-.613.43-.991a6.932 6.932 0 0 1 0-.255c.007-.38-.138-.751-.43-.992l-1.004-.827a1.125 1.125 0 0 1-.26-1.43l1.297-2.247a1.125 1.125 0 0 1 1.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.086.22-.128.332-.183.582-.495.644-.869l.214-1.28Z" />
                <path strokeLinecap="round" strokeLinejoin="round" d="M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z" />
              </svg>
            </button>
          </div>
        </div>
      </header>

      {/* Main Content */}
      <div className="flex-1 flex">
        {view === 'search' && (
          <aside className="w-56 border-r border-sage-100 bg-sage-50/30 p-6">
            <details className="group" open>
              <summary className="text-xs font-semibold text-sage-400 mb-4 uppercase tracking-widest cursor-pointer hover:text-sage-600 transition-colors flex items-center gap-2 select-none">
                <svg className="w-3 h-3 transition-transform group-open:rotate-90" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={3} d="M9 5l7 7-7 7" />
                </svg>
                Tags
              </summary>
              <div className="mt-2">
                <TagFilter selectedTag={selectedTag} onSelectTag={handleSelectTag} />
              </div>
            </details>

            <div className="mt-6">
              <Rediscover
                onTagClick={(tag) => {
                  setSelectedTag(tag)
                  setView('search')
                }}
                onNoteExpand={setExpandedNote}
              />
            </div>
          </aside>
        )}

        <main className="flex-1 overflow-y-auto">
          <div className="mx-auto max-w-4xl px-6 lg:px-8 py-8 pb-20">
            {view === 'settings' ? (
              <Settings />
            ) : view === 'chat' ? (
              <div className="h-[calc(100vh-12rem)]">
                <ChatInterface />
              </div>
            ) : (
              <>
                {showWelcome && (
                  <WelcomeBack onDismiss={() => {
                    setShowWelcome(false)
                    sessionStorage.setItem('welcome_dismissed', '1')
                  }} />
                )}
                <CaptureInput onCapture={handleCapture} onIngestUrl={handleIngestUrl} disabled={saving} ingesting={ingesting} inline />
                <SearchBar value={query} onChange={setQuery} />

                {/* Show/Hide Imported Items Toggle */}
                <div className="mt-4 flex items-center justify-end gap-3">
                  <label htmlFor="hide-imported-toggle" className="text-sm text-sage-600 font-medium">
                    Hide imported items
                  </label>
                  <button
                    id="hide-imported-toggle"
                    role="switch"
                    aria-checked={hideImportedItems}
                    onClick={() => setHideImportedItems(!hideImportedItems)}
                    className={`
                      relative inline-flex h-6 w-11 items-center rounded-full transition-colors
                      ${hideImportedItems ? 'bg-sage-600' : 'bg-sage-300'}
                    `}
                  >
                    <span
                      className={`
                        inline-block h-4 w-4 transform rounded-full bg-white transition-transform
                        ${hideImportedItems ? 'translate-x-6' : 'translate-x-1'}
                      `}
                    />
                  </button>
                  <span className="text-xs text-sage-400">
                    {hideImportedItems ? '(Homebrew, bookmarks hidden)' : '(Showing all)'}
                  </span>
                </div>

                {/* Active Filter Indicator */}
                {selectedTag && (
                  <div className="mt-6 flex items-center gap-3">
                    <span className="text-xs uppercase tracking-widest text-sage-400 font-semibold">Filtered by</span>
                    <button
                      onClick={() => setSelectedTag(null)}
                      className="inline-flex items-center gap-2 px-4 py-2 bg-sage-100 text-sage-700 rounded-full text-sm font-medium hover:bg-sage-200 transition-all group"
                    >
                      <span>{selectedTag}</span>
                      <svg
                        className="w-3.5 h-3.5 text-sage-400 group-hover:text-sage-600 transition-colors"
                        fill="none"
                        viewBox="0 0 24 24"
                        stroke="currentColor"
                      >
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M6 18L18 6M6 6l12 12" />
                      </svg>
                    </button>
                  </div>
                )}

                <div className="mt-8">
                  {loading && notes.length === 0 && (
                    <div className="space-y-4">
                      <NoteSkeleton />
                      <NoteSkeleton />
                      <NoteSkeleton />
                    </div>
                  )}

                  {error && (
                    <div className="rounded-xl bg-muted-red/10 border border-muted-red/20 p-6 text-center">
                      <p className="text-muted-red">{error}</p>
                    </div>
                  )}

                  {!loading && !error && notes.length === 0 && (
                    <div className="rounded-xl bg-sage-50/50 border border-sage-100 p-12 text-center">
                      <p className="text-sage-400 font-light">
                        {selectedTag
                          ? `No notes tagged with "${selectedTag}".`
                          : query
                          ? 'No notes match your search.'
                          : 'No notes yet. Capture your first thought below.'}
                      </p>
                    </div>
                  )}

                  {!loading && !error && notes.length > 0 && (
                    <>
                      {/* Note count */}
                      {(query || selectedTag) && (
                        <div className="mb-4 text-sm text-sage-500">
                          {notes.length} {notes.length === 1 ? 'note' : 'notes'} found
                        </div>
                      )}
                      {!query && !selectedTag ? (
                        <DailyDigest
                          notes={notes}
                          onNoteDeleted={() => loadNotes(query, selectedTag, hideImportedItems)}
                          onTagClick={(tag) => {
                            setSelectedTag(tag)
                            setView('search')
                          }}
                          onNoteExpand={setExpandedNote}
                          onAudioGenerated={setPlayEpisodeId}
                        />
                      ) : (
                        <div className="space-y-4">
                          {notes.map((note) => (
                            <NoteCard
                              key={note.id}
                              note={note}
                              searchQuery={query}
                              onDelete={() => loadNotes(query, selectedTag, hideImportedItems)}
                              onTagClick={(tag) => {
                                setSelectedTag(tag)
                                setView('search')
                              }}
                              onExpand={setExpandedNote}
                              onAudioGenerated={setPlayEpisodeId}
                            />
                          ))}
                        </div>
                      )}
                    </>
                  )}
                </div>
              </>
            )}
          </div>
        </main>
      </div>

      {/* Global modal capture (⌘K) - works in both views */}
      <CaptureInput onCapture={handleCapture} onIngestUrl={handleIngestUrl} disabled={saving} ingesting={ingesting} />

      {/* Voice recorder FAB - works in both views */}
      <VoiceRecorder
        onComplete={() => loadNotes(query, selectedTag, hideImportedItems)}
        disabled={saving}
      />

      {/* Side Panel for expanded notes */}
      {expandedNote && (
        <NoteSidePanel
          note={expandedNote}
          searchQuery={query}
          onClose={() => {
            setExpandedNote(null)
            setDeepLinkNoteId(null)
            const raw = window.location.hash.replace(/^#\/?/, '')
            const [path] = raw.split('?')
            if (path === 'search' || path === 'settings') {
              window.location.hash = `/${path}`
            }
          }}
        />
      )}

      {/* Audio Player */}
      <AudioPlayer
        playEpisodeId={playEpisodeId}
        onClearPlay={() => setPlayEpisodeId(null)}
      />
    </div>
    </TokenGate>
  )
}
