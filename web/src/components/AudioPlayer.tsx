import { useState, useRef, useEffect, useCallback } from 'react'
import { fetchEpisodes, deleteEpisode, audioFileUrl, type AudioEpisode } from '../api'

function formatTime(secs: number): string {
  if (!isFinite(secs) || secs < 0) return '0:00'
  const m = Math.floor(secs / 60)
  const s = Math.floor(secs % 60)
  return `${m}:${s.toString().padStart(2, '0')}`
}

const SPEED_OPTIONS = [0.75, 1, 1.25, 1.5, 1.75, 2]

interface AudioPlayerProps {
  /** When set, immediately start playing this episode */
  playEpisodeId?: number | null
  onClearPlay?: () => void
}

export function AudioPlayer({ playEpisodeId, onClearPlay }: AudioPlayerProps) {
  const [episodes, setEpisodes] = useState<AudioEpisode[]>([])
  const [showList, setShowList] = useState(false)
  const [current, setCurrent] = useState<AudioEpisode | null>(null)
  const [isPlaying, setIsPlaying] = useState(false)
  const [progress, setProgress] = useState(0)
  const [duration, setDuration] = useState(0)
  const [speed, setSpeed] = useState(1)
  const [showSpeed, setShowSpeed] = useState(false)
  const audioRef = useRef<HTMLAudioElement | null>(null)
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const loadEpisodes = useCallback(async () => {
    try {
      const res = await fetchEpisodes()
      setEpisodes(res.episodes)
    } catch {
      // silent
    }
  }, [])

  useEffect(() => {
    queueMicrotask(() => void loadEpisodes())
  }, [loadEpisodes])

  // Poll for pending episodes
  useEffect(() => {
    const hasPending = episodes.some((e) => e.status === 'pending' || e.status === 'processing')
    if (hasPending) {
      pollRef.current = setInterval(loadEpisodes, 5000)
    }
    return () => {
      if (pollRef.current) clearInterval(pollRef.current)
    }
  }, [episodes, loadEpisodes])

  // When a new episode is triggered, reload the list immediately so it appears
  useEffect(() => {
    if (playEpisodeId != null) {
      queueMicrotask(() => void loadEpisodes())
      onClearPlay?.()
    }
  }, [loadEpisodes, onClearPlay, playEpisodeId])

  // Close speed menu on outside click
  useEffect(() => {
    if (!showSpeed) return
    function handleClick() { setShowSpeed(false) }
    document.addEventListener('click', handleClick)
    return () => document.removeEventListener('click', handleClick)
  }, [showSpeed])

  function saveProgress(epId: number, time: number) {
    localStorage.setItem(`audio_progress_${epId}`, String(time))
  }

  function loadProgress(epId: number): number {
    return parseFloat(localStorage.getItem(`audio_progress_${epId}`) || '0') || 0
  }

  function play(ep: AudioEpisode) {
    if (audioRef.current) {
      audioRef.current.pause()
    }
    const saved = loadProgress(ep.id)
    const audio = new Audio(audioFileUrl(ep.id))
    audio.playbackRate = speed
    audio.addEventListener('loadedmetadata', () => {
      setDuration(audio.duration)
      if (saved > 0 && saved < audio.duration - 1) {
        audio.currentTime = saved
      }
    })
    audio.addEventListener('timeupdate', () => {
      setProgress(audio.currentTime)
      // Save every ~5 seconds (timeupdate fires ~4x/sec)
      if (Math.floor(audio.currentTime) % 5 === 0) {
        saveProgress(ep.id, audio.currentTime)
      }
    })
    audio.addEventListener('ended', () => {
      setIsPlaying(false)
      setProgress(0)
      localStorage.removeItem(`audio_progress_${ep.id}`)
    })
    audio.addEventListener('pause', () => {
      saveProgress(ep.id, audio.currentTime)
    })
    audioRef.current = audio
    setCurrent(ep)
    setIsPlaying(true)
    setProgress(saved)
    audio.play()
  }

  function togglePlayPause() {
    if (!audioRef.current) return
    if (isPlaying) {
      audioRef.current.pause()
      setIsPlaying(false)
    } else {
      audioRef.current.play()
      setIsPlaying(true)
    }
  }

  function skipBack() {
    if (!audioRef.current) return
    audioRef.current.currentTime = Math.max(0, audioRef.current.currentTime - 10)
  }

  function skipForward() {
    if (!audioRef.current) return
    audioRef.current.currentTime = Math.min(audioRef.current.duration || 0, audioRef.current.currentTime + 30)
  }

  function changeSpeed(newSpeed: number) {
    setSpeed(newSpeed)
    if (audioRef.current) {
      audioRef.current.playbackRate = newSpeed
    }
    setShowSpeed(false)
  }

  function seek(e: React.MouseEvent<HTMLDivElement>) {
    if (!audioRef.current || !duration) return
    const rect = e.currentTarget.getBoundingClientRect()
    const pct = (e.clientX - rect.left) / rect.width
    audioRef.current.currentTime = pct * duration
  }

  async function handleDelete(ep: AudioEpisode) {
    if (!confirm(`Delete "${ep.title}"?`)) return
    try {
      await deleteEpisode(ep.id)
      if (current?.id === ep.id) {
        audioRef.current?.pause()
        setCurrent(null)
        setIsPlaying(false)
      }
      await loadEpisodes()
    } catch {
      // silent
    }
  }

  const completeEpisodes = episodes.filter((e) => e.status === 'complete')
  const pendingEpisodes = episodes.filter((e) => e.status === 'pending' || e.status === 'processing')
  const failedEpisodes = episodes.filter((e) => e.status === 'failed')

  if (episodes.length === 0) return null

  return (
    <>
      {/* Mini player bar — fixed at bottom */}
      <div className="fixed bottom-0 left-0 right-0 z-40 border-t border-sage-200 bg-white/95 backdrop-blur-sm shadow-lg">
        <div className="mx-auto max-w-7xl px-4 py-2 flex items-center gap-2">
          {/* Toggle episode list */}
          <button
            onClick={() => setShowList(!showList)}
            className="p-2 rounded-full hover:bg-sage-100 text-sage-500 transition-colors"
            title={showList ? 'Hide episodes' : 'Show episodes'}
          >
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M3.75 6.75h16.5M3.75 12h16.5m-16.5 5.25h16.5" />
            </svg>
          </button>

          {/* Skip back 10s */}
          <button
            onClick={skipBack}
            disabled={!current}
            className="p-1.5 rounded-full hover:bg-sage-100 text-sage-500 disabled:opacity-30 transition-colors"
            title="Back 10 seconds"
          >
            <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M9 15L3 9m0 0l6-6M3 9h12a6 6 0 010 12h-3" />
            </svg>
            <span className="sr-only">-10s</span>
          </button>

          {/* Play/pause */}
          <button
            onClick={togglePlayPause}
            disabled={!current}
            className="p-2 rounded-full bg-sage-600 text-white hover:bg-sage-700 disabled:opacity-40 transition-colors"
          >
            {isPlaying ? (
              <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                <rect x="6" y="5" width="4" height="14" rx="1" />
                <rect x="14" y="5" width="4" height="14" rx="1" />
              </svg>
            ) : (
              <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                <path d="M8 5.14v14l11-7-11-7z" />
              </svg>
            )}
          </button>

          {/* Skip forward 30s */}
          <button
            onClick={skipForward}
            disabled={!current}
            className="p-1.5 rounded-full hover:bg-sage-100 text-sage-500 disabled:opacity-30 transition-colors"
            title="Forward 30 seconds"
          >
            <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M15 15l6-6m0 0l-6-6m6 6H9a6 6 0 000 12h3" />
            </svg>
            <span className="sr-only">+30s</span>
          </button>

          {/* Track info + progress */}
          <div className="flex-1 min-w-0">
            {current ? (
              <>
                <div className="text-sm font-medium text-sage-800 truncate">{current.title}</div>
                <div className="flex items-center gap-2 mt-1">
                  <span className="text-xs text-sage-400 tabular-nums w-9 text-right">{formatTime(progress)}</span>
                  <div className="flex-1 py-2 cursor-pointer" onClick={seek}>
                    <div className="h-1.5 bg-sage-100 rounded-full">
                      <div
                        className="h-full bg-sage-500 rounded-full transition-[width] duration-200"
                        style={{ width: duration ? `${(progress / duration) * 100}%` : '0%' }}
                      />
                    </div>
                  </div>
                  <span className="text-xs text-sage-400 tabular-nums w-9">{formatTime(duration)}</span>
                </div>
              </>
            ) : (
              <div className="text-sm text-sage-400">
                {pendingEpisodes.length > 0
                  ? `${pendingEpisodes.length} episode${pendingEpisodes.length > 1 ? 's' : ''} generating...`
                  : 'Select an episode to play'}
              </div>
            )}
          </div>

          {/* Speed control */}
          <div className="relative">
            <button
              onClick={(e) => { e.stopPropagation(); setShowSpeed(!showSpeed) }}
              disabled={!current}
              className="px-2 py-1 rounded-md text-xs font-medium text-sage-600 hover:bg-sage-100 disabled:opacity-30 transition-colors tabular-nums"
              title="Playback speed"
            >
              {speed === 1 ? '1x' : `${speed}x`}
            </button>
            {showSpeed && (
              <div className="absolute bottom-full right-0 mb-2 bg-white border border-sage-200 rounded-lg shadow-xl py-1 min-w-[4rem]">
                {SPEED_OPTIONS.map((s) => (
                  <button
                    key={s}
                    onClick={(e) => { e.stopPropagation(); changeSpeed(s) }}
                    className={`block w-full text-left px-3 py-1.5 text-xs tabular-nums transition-colors ${
                      s === speed ? 'bg-sage-100 text-sage-800 font-semibold' : 'text-sage-600 hover:bg-sage-50'
                    }`}
                  >
                    {s}x
                  </button>
                ))}
              </div>
            )}
          </div>

          {/* Pending indicator */}
          {pendingEpisodes.length > 0 && (
            <div className="flex items-center gap-1.5 text-xs text-amber-600">
              <svg className="w-4 h-4 animate-spin" fill="none" viewBox="0 0 24 24">
                <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
              </svg>
              <span>{pendingEpisodes.length}</span>
            </div>
          )}
        </div>
      </div>

      {/* Episode list panel */}
      {showList && (
        <div className="fixed bottom-14 left-0 right-0 z-30 max-h-80 overflow-y-auto border-t border-sage-200 bg-white/98 backdrop-blur-sm shadow-xl">
          <div className="mx-auto max-w-7xl px-4 py-3">
            <h3 className="text-xs font-semibold text-sage-400 uppercase tracking-widest mb-3">Episodes</h3>

            {completeEpisodes.length === 0 && pendingEpisodes.length === 0 && failedEpisodes.length === 0 && (
              <p className="text-sm text-sage-400 py-4 text-center">No episodes yet</p>
            )}

            <div className="space-y-1">
              {pendingEpisodes.map((ep) => (
                <div key={ep.id} className="flex items-center gap-3 px-3 py-2 rounded-lg bg-amber-50/50">
                  <svg className="w-4 h-4 animate-spin text-amber-500 shrink-0" fill="none" viewBox="0 0 24 24">
                    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                  </svg>
                  <div className="flex-1 min-w-0">
                    <div className="text-sm text-sage-700 truncate">{ep.title}</div>
                    <div className="text-xs text-amber-600">Generating...</div>
                  </div>
                </div>
              ))}

              {completeEpisodes.map((ep) => (
                <div
                  key={ep.id}
                  className={`flex items-center gap-3 px-3 py-2 rounded-lg cursor-pointer transition-colors ${
                    current?.id === ep.id ? 'bg-sage-100' : 'hover:bg-sage-50'
                  }`}
                  onClick={() => play(ep)}
                >
                  {current?.id === ep.id && isPlaying ? (
                    <svg className="w-4 h-4 text-sage-600 shrink-0" fill="currentColor" viewBox="0 0 24 24">
                      <rect x="6" y="5" width="4" height="14" rx="1" />
                      <rect x="14" y="5" width="4" height="14" rx="1" />
                    </svg>
                  ) : (
                    <svg className="w-4 h-4 text-sage-400 shrink-0" fill="currentColor" viewBox="0 0 24 24">
                      <path d="M8 5.14v14l11-7-11-7z" />
                    </svg>
                  )}
                  <div className="flex-1 min-w-0">
                    <div className="text-sm text-sage-800 truncate">{ep.title}</div>
                    <div className="text-xs text-sage-400">
                      {ep.episode_type} &middot; {ep.tts_provider}
                      {ep.duration_seconds != null && ` \u00B7 ${formatTime(ep.duration_seconds)}`}
                    </div>
                  </div>
                  <button
                    onClick={(e) => {
                      e.stopPropagation()
                      handleDelete(ep)
                    }}
                    className="p-1 text-sage-300 hover:text-muted-red transition-colors"
                    title="Delete episode"
                  >
                    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                    </svg>
                  </button>
                </div>
              ))}

              {failedEpisodes.map((ep) => (
                <div key={ep.id} className="flex items-center gap-3 px-3 py-2 rounded-lg bg-rose-50/50">
                  <svg className="w-4 h-4 text-muted-red shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4.5c-.77-.833-2.694-.833-3.464 0L3.34 16.5c-.77.833.192 2.5 1.732 2.5z" />
                  </svg>
                  <div className="flex-1 min-w-0">
                    <div className="text-sm text-sage-700 truncate">{ep.title}</div>
                    <div className="text-xs text-muted-red">{ep.error_message || 'Generation failed'}</div>
                  </div>
                  <button
                    onClick={() => handleDelete(ep)}
                    className="p-1 text-sage-300 hover:text-muted-red transition-colors"
                    title="Delete episode"
                  >
                    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  </button>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}
    </>
  )
}
