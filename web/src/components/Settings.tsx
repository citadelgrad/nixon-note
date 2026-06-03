import { useEffect, useRef, useState } from 'react'
import { fetchStatus, fetchUsageSummary, fetchSettings, updateSettings, type StatusResponse, type ServiceStatus, type UsageSummary, type AppSettings } from '../api'

const SERVICE_META: Record<string, { label: string; description: string }> = {
  ollama: {
    label: 'Ollama Embeddings',
    description: 'Vector embeddings for semantic search. Falls back to full-text search (FTS5) when unavailable.',
  },
  anthropic: {
    label: 'Anthropic Claude',
    description: 'Auto-tagging and summarization of notes on capture.',
  },
  gemini: {
    label: 'Google Gemini',
    description: 'Conversational chat interface and YouTube transcript summarization.',
  },
  whisper: {
    label: 'Whisper Transcription',
    description: 'Local voice-to-text transcription using OpenAI Whisper models.',
  },
  web_clip: {
    label: 'Web Clipper',
    description: 'Article extraction from URLs, converts web pages to Markdown notes.',
  },
  openai_tts: {
    label: 'OpenAI TTS',
    description: 'Text-to-speech audio generation using OpenAI voices.',
  },
  elevenlabs_tts: {
    label: 'ElevenLabs TTS',
    description: 'Text-to-speech audio generation using business-ready ElevenLabs voices.',
  },
}

const TTS_PROVIDER_META: Record<string, { label: string; desc: string; serviceKey: string; envKey: string }> = {
  openai: { label: 'OpenAI', desc: 'High quality, fast', serviceKey: 'openai_tts', envKey: 'OPENAI_API_KEY' },
  gemini: { label: 'Gemini', desc: 'Google TTS', serviceKey: 'gemini', envKey: 'GEMINI_API_KEY' },
  elevenlabs: { label: 'ElevenLabs', desc: 'Business voices', serviceKey: 'elevenlabs_tts', envKey: 'ELEVENLABS_API_KEY' },
}

const OPENAI_VOICES = ['alloy', 'echo', 'fable', 'nova', 'onyx', 'shimmer']
const GEMINI_VOICES = ['Kore', 'Charon', 'Fenrir', 'Aoede', 'Puck', 'Leda']
const OPENAI_VOICE_PREVIEW_URL = 'https://www.openai.fm/'
const GEMINI_VOICE_PREVIEW_URL = 'https://aistudio.google.com/generate-speech'
const elevenLabsVoicePreviewUrl = (voiceId: string) => `https://elevenlabs.io/app/voice-library?voiceId=${encodeURIComponent(voiceId)}`
const ELEVENLABS_VOICES = [
  { name: 'The Financial Strategist', id: 'N2lVS1wzUtoSnaSjtS9X', vibe: 'The CFO. Crisp, no-nonsense, refined. Best for earnings and market analysis.' },
  { name: 'Nicole', id: 'piTKgcLEGmPE4e6mEKli', vibe: 'Professional narrator. Stable, clear, strong with jargon like EBITDA and Quantitative Easing.' },
  { name: 'The Visionary Coach', id: 'ExT4S6mB5oP9Q1R2S3T4', vibe: 'Thought leader. Smooth, melodic, inspiring but grounded.' },
  { name: 'Alice', id: 'Xb7hH8MSUJpSbSDYk0k2', vibe: 'News desk. Confident British voice for market updates and professional news.' },
  { name: 'Clayton', id: 'fQ9aRKjmL75dgjNakj2u', vibe: 'Business closer. Executive tone for B2B or sales-heavy content.' },
  { name: 'Bill L. Oxley', id: 'onwK4e9ZLuTAKqWW03F9', vibe: 'Market watch. British financial-news-anchor gravitas.' },
  { name: 'Marcus', id: 'aDYxt2YzboRX5QmntZNE', vibe: 'Diplomat. Measured, calm, articulate; good for investing deep dives.' },
  { name: 'Andray', id: 'FsK9b8Cv2pGkFUtfpOyM', vibe: 'Approachable expert. Friendly professional tone for how-to investing guides.' },
]

function StatusDot({ status }: { status: ServiceStatus }) {
  if (!status.configured) {
    return <span className="w-2.5 h-2.5 rounded-full bg-sage-300 inline-block" title="Not Configured" />
  }
  if (status.healthy) {
    return <span className="w-2.5 h-2.5 rounded-full bg-emerald-400 inline-block" title="Connected" />
  }
  return <span className="w-2.5 h-2.5 rounded-full bg-muted-red inline-block" title="Unavailable" />
}

function statusText(status: ServiceStatus): string {
  if (!status.configured) return 'Not Configured'
  return status.healthy ? 'Connected' : 'Unavailable'
}

function ServiceCard({ name, status }: { name: string; status: ServiceStatus }) {
  const meta = SERVICE_META[name] ?? { label: name, description: '' }

  return (
    <div className="rounded-xl border border-sage-100 bg-white p-5">
      <div className="flex items-center gap-3 mb-2">
        <StatusDot status={status} />
        <h3 className="text-sm font-medium text-sage-800">{meta.label}</h3>
        <span className={`ml-auto text-xs font-medium ${
          !status.configured ? 'text-sage-400' : status.healthy ? 'text-emerald-700' : 'text-muted-red'
        }`}>
          {statusText(status)}
        </span>
      </div>
      <p className="text-xs font-light text-sage-500 mb-3">{meta.description}</p>
      <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-sage-600">
        {status.url && <span><span className="text-sage-400">URL:</span> {status.url}</span>}
        {status.model && <span><span className="text-sage-400">Model:</span> {status.model}</span>}
        {status.model_size && <span><span className="text-sage-400">Size:</span> {status.model_size}</span>}
        {status.model_dir && <span><span className="text-sage-400">Dir:</span> {status.model_dir}</span>}
      </div>
    </div>
  )
}

export function Settings() {
  const [data, setData] = useState<StatusResponse | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const intervalRef = useRef<ReturnType<typeof setInterval>>(null)
  const [theme, setTheme] = useState(() => localStorage.getItem('note_theme') ?? 'sage')
  const [usage, setUsage] = useState<UsageSummary[]>([])
  const [appSettings, setAppSettings] = useState<AppSettings>({})
  const [savingSettings, setSavingSettings] = useState(false)
  const [copiedVoiceId, setCopiedVoiceId] = useState<string | null>(null)

  function changeTheme(newTheme: string) {
    document.documentElement.setAttribute('data-theme', newTheme)
    localStorage.setItem('note_theme', newTheme)
    setTheme(newTheme)
  }

  async function copyVoiceId(voiceId: string) {
    await window.navigator.clipboard.writeText(voiceId)
    setCopiedVoiceId(voiceId)
    window.setTimeout(() => setCopiedVoiceId((current) => current === voiceId ? null : current), 1500)
  }

  useEffect(() => {
    let cancelled = false

    async function load() {
      try {
        const [statusRes, usageRes, settingsRes] = await Promise.all([
          fetchStatus(),
          fetchUsageSummary().catch(() => ({ summary: [] })),
          fetchSettings().catch(() => ({} as AppSettings)),
        ])
        if (!cancelled) {
          setData(statusRes)
          setUsage(usageRes.summary)
          setAppSettings(settingsRes)
          setError(null)
        }
      } catch (e) {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : 'Failed to load status')
        }
      } finally {
        if (!cancelled) setLoading(false)
      }
    }

    load()
    intervalRef.current = setInterval(load, 30_000)

    return () => {
      cancelled = true
      if (intervalRef.current) clearInterval(intervalRef.current)
    }
  }, [])

  if (loading) {
    return (
      <div className="space-y-6">
        <div className="text-xs font-semibold text-sage-400 uppercase tracking-widest">Services</div>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {[1, 2, 3, 4, 5].map((i) => (
            <div key={i} className="rounded-xl border border-sage-100 bg-white p-5 animate-pulse">
              <div className="h-4 bg-sage-100 rounded w-1/3 mb-3" />
              <div className="h-3 bg-sage-50 rounded w-2/3 mb-2" />
              <div className="h-3 bg-sage-50 rounded w-1/2" />
            </div>
          ))}
        </div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="rounded-xl bg-muted-red/10 border border-muted-red/20 p-6 text-center">
        <p className="text-muted-red">{error}</p>
      </div>
    )
  }

  if (!data) return null

  const serviceEntries = Object.entries(data.services)

  return (
    <div className="space-y-10">
      {/* Theme Section */}
      <section>
        <h2 className="text-xs font-semibold text-sage-400 uppercase tracking-widest mb-5">Theme</h2>
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
          {[
            { id: 'sage', label: 'Sage', colors: ['#6b7d5a', '#e8ebe3', '#f6f7f4'] },
            { id: 'desert', label: 'Desert', colors: ['#8b6914', '#e8ddd0', '#f5ede4'] },
            { id: 'sunset', label: 'Sunset', colors: ['#c05234', '#f0d8cf', '#fceee8'] },
            { id: 'forest', label: 'Forest', colors: ['#3d6b2e', '#c8d8ba', '#e8ede0'] },
          ].map((t) => (
            <button
              key={t.id}
              onClick={() => changeTheme(t.id)}
              className={`p-4 rounded-xl border-2 transition-all ${
                theme === t.id ? 'border-sage-500 shadow-sm' : 'border-sage-100 hover:border-sage-300'
              }`}
            >
              <div className="flex gap-1 mb-2">
                {t.colors.map((c, i) => (
                  <div key={i} className="w-5 h-5 rounded-full" style={{ backgroundColor: c }} />
                ))}
              </div>
              <span className="text-xs font-medium text-sage-700">{t.label}</span>
            </button>
          ))}
        </div>
      </section>

      {/* Text-to-Speech Section */}
      <section>
        <h2 className="text-xs font-semibold text-sage-400 uppercase tracking-widest mb-5">Text-to-Speech</h2>
        {(() => {
          const provider = appSettings.tts_provider || 'openai'
          const providerMeta = TTS_PROVIDER_META[provider] ?? TTS_PROVIDER_META.openai
          const svc = data.services[providerMeta.serviceKey]
          if (svc && !svc.healthy) {
            return (
              <div className="mb-4 px-4 py-3 rounded-lg bg-amber-50 border border-amber-200 text-sm text-amber-800">
                {!svc.configured
                  ? `${providerMeta.label} TTS is not configured. Set ${providerMeta.envKey} to enable audio generation.`
                  : `${providerMeta.label} TTS service is currently unavailable.`}
              </div>
            )
          }
          return null
        })()}
        <div className="rounded-xl border border-sage-100 bg-white p-5 space-y-5">
          {/* Provider */}
          <div>
            <label className="text-sm font-medium text-sage-700 block mb-2">TTS Provider</label>
            <div className="flex gap-3">
              {Object.entries(TTS_PROVIDER_META).map(([id, p]) => (
                <button
                  key={id}
                  onClick={() => setAppSettings((s) => ({ ...s, tts_provider: id }))}
                  className={`flex-1 p-3 rounded-lg border-2 transition-all text-left ${
                    (appSettings.tts_provider || 'openai') === id
                      ? 'border-sage-500 bg-sage-50'
                      : 'border-sage-100 hover:border-sage-300'
                  }`}
                >
                  <div className="text-sm font-medium text-sage-800">{p.label}</div>
                  <div className="text-xs text-sage-500">{p.desc}</div>
                </button>
              ))}
            </div>
          </div>

          {/* Voice — changes based on provider */}
          <div>
            <label className="text-sm font-medium text-sage-700 block mb-2">Voice</label>
            {(appSettings.tts_provider || 'openai') === 'openai' ? (
              <div className="grid grid-cols-3 gap-2">
                {OPENAI_VOICES.map((v) => {
                  const selected = (appSettings.tts_voice_openai || 'alloy') === v
                  return (
                    <div
                      key={v}
                      className={`rounded-lg border transition-all ${
                        selected
                          ? 'border-sage-500 bg-sage-50 text-sage-800 font-medium'
                          : 'border-sage-100 text-sage-600 hover:border-sage-300'
                      }`}
                    >
                      <button
                        type="button"
                        onClick={() => setAppSettings((s) => ({ ...s, tts_voice_openai: v }))}
                        className="w-full px-3 py-2 text-sm capitalize text-left"
                      >
                        {v}
                      </button>
                      <div className="border-t border-sage-100 px-3 py-2">
                        <a
                          href={OPENAI_VOICE_PREVIEW_URL}
                          target="_blank"
                          rel="noreferrer"
                          className="text-xs font-medium text-sage-700 hover:text-sage-900 underline underline-offset-2"
                        >
                          Hear example on OpenAI.fm ↗
                        </a>
                      </div>
                    </div>
                  )
                })}
              </div>
            ) : (appSettings.tts_provider || 'openai') === 'elevenlabs' ? (
              <div className="space-y-3">
                {(() => {
                  const customName = appSettings.tts_voice_elevenlabs_custom_name || 'Custom ElevenLabs voice'
                  const customId = appSettings.tts_voice_elevenlabs_custom_id || ''
                  const selected = customId !== '' && appSettings.tts_voice_elevenlabs === customId
                  return (
                    <div
                      className={`rounded-lg border transition-all p-3 ${
                        selected
                          ? 'border-sage-500 bg-sage-50 text-sage-800'
                          : 'border-sage-100 text-sage-600'
                      }`}
                    >
                      <div className="flex items-start justify-between gap-3 mb-3">
                        <div>
                          <div className="text-sm font-medium text-sage-800">Custom ElevenLabs voice</div>
                          <div className="text-xs text-sage-500 mt-1">Paste any ElevenLabs voice name and ID here. Saving uses this ID for audio generation.</div>
                        </div>
                        <button
                          type="button"
                          disabled={!customId.trim()}
                          onClick={() => setAppSettings((s) => ({ ...s, tts_voice_elevenlabs: customId.trim() }))}
                          className={`shrink-0 px-3 py-2 rounded-md border text-xs font-medium transition-all ${
                            selected
                              ? 'border-sage-500 bg-white text-sage-800'
                              : 'border-sage-200 bg-white text-sage-700 hover:border-sage-400 disabled:opacity-50 disabled:cursor-not-allowed'
                          }`}
                        >
                          Use custom ElevenLabs voice
                        </button>
                      </div>
                      <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                        <label className="block">
                          <span className="text-xs font-medium text-sage-600">Custom ElevenLabs voice name</span>
                          <input
                            type="text"
                            value={customName === 'Custom ElevenLabs voice' ? '' : customName}
                            onChange={(e) => setAppSettings((s) => ({ ...s, tts_voice_elevenlabs_custom_name: e.target.value }))}
                            placeholder="Nicole, Sam, your cloned voice..."
                            className="mt-1 w-full rounded-md border border-sage-200 bg-white px-3 py-2 text-sm text-sage-800 focus:border-sage-500 focus:outline-none"
                          />
                        </label>
                        <label className="block">
                          <span className="text-xs font-medium text-sage-600">Custom ElevenLabs voice ID</span>
                          <input
                            type="text"
                            value={customId}
                            onChange={(e) => {
                              const nextId = e.target.value.trim()
                              setAppSettings((s) => ({
                                ...s,
                                tts_voice_elevenlabs_custom_id: nextId,
                                tts_voice_elevenlabs: nextId || s.tts_voice_elevenlabs,
                              }))
                            }}
                            placeholder="piTKgcLEGmPE4e6mEKli"
                            className="mt-1 w-full rounded-md border border-sage-200 bg-white px-3 py-2 font-mono text-sm text-sage-800 focus:border-sage-500 focus:outline-none"
                          />
                        </label>
                      </div>
                      <div className="mt-3 flex flex-wrap items-center gap-3 text-xs">
                        {customId ? (
                          <button
                            type="button"
                            onClick={() => void copyVoiceId(customId)}
                            className="font-mono text-sage-600 hover:text-sage-900 underline underline-offset-2"
                            aria-label="Copy custom ElevenLabs voice ID"
                          >
                            {copiedVoiceId === customId ? 'Copied!' : `Selected ID: ${customId}`}
                          </button>
                        ) : <span className="text-sage-400">Enter an ID to use this voice.</span>}
                        {customId && (
                          <a
                            href={elevenLabsVoicePreviewUrl(customId)}
                            target="_blank"
                            rel="noreferrer"
                            className="font-medium text-sage-700 hover:text-sage-900 underline underline-offset-2"
                          >
                            Preview in ElevenLabs ↗
                          </a>
                        )}
                      </div>
                    </div>
                  )
                })()}
                <div className="grid grid-cols-1 md:grid-cols-2 gap-2">
                  {ELEVENLABS_VOICES.map((v) => {
                    const selected = (appSettings.tts_voice_elevenlabs || 'N2lVS1wzUtoSnaSjtS9X') === v.id
                    return (
                      <div
                        key={v.id}
                        className={`rounded-lg border transition-all ${
                          selected
                            ? 'border-sage-500 bg-sage-50 text-sage-800'
                            : 'border-sage-100 text-sage-600 hover:border-sage-300'
                        }`}
                      >
                        <button
                          type="button"
                          onClick={() => setAppSettings((s) => ({ ...s, tts_voice_elevenlabs: v.id }))}
                          className="w-full p-3 text-left"
                        >
                          <div className="text-sm font-medium">{v.name}</div>
                          <div className="text-xs text-sage-500 mt-2 leading-relaxed">{v.vibe}</div>
                        </button>
                        <div className="border-t border-sage-100 px-3 py-2 flex flex-wrap items-center gap-3">
                          <button
                            type="button"
                            onClick={() => void copyVoiceId(v.id)}
                            className="text-xs text-sage-500 hover:text-sage-900 font-mono underline underline-offset-2"
                            aria-label={`Copy ${v.name} voice ID`}
                          >
                            {copiedVoiceId === v.id ? 'Copied!' : v.id}
                          </button>
                          <a
                            href={elevenLabsVoicePreviewUrl(v.id)}
                            target="_blank"
                            rel="noreferrer"
                            className="text-xs font-medium text-sage-700 hover:text-sage-900 underline underline-offset-2"
                          >
                            Preview in ElevenLabs ↗
                          </a>
                        </div>
                      </div>
                    )
                  })}
                </div>
              </div>
            ) : (
              <div className="grid grid-cols-3 gap-2">
                {GEMINI_VOICES.map((v) => {
                  const selected = (appSettings.tts_voice_gemini || 'Kore') === v
                  return (
                    <div
                      key={v}
                      className={`rounded-lg border transition-all ${
                        selected
                          ? 'border-sage-500 bg-sage-50 text-sage-800 font-medium'
                          : 'border-sage-100 text-sage-600 hover:border-sage-300'
                      }`}
                    >
                      <button
                        type="button"
                        onClick={() => setAppSettings((s) => ({ ...s, tts_voice_gemini: v }))}
                        className="w-full px-3 py-2 text-sm text-left"
                      >
                        {v}
                      </button>
                      <div className="border-t border-sage-100 px-3 py-2">
                        <a
                          href={GEMINI_VOICE_PREVIEW_URL}
                          target="_blank"
                          rel="noreferrer"
                          className="text-xs font-medium text-sage-700 hover:text-sage-900 underline underline-offset-2"
                        >
                          Hear example in AI Studio ↗
                        </a>
                      </div>
                    </div>
                  )
                })}
              </div>
            )}
          </div>

          {(appSettings.tts_provider || 'openai') === 'elevenlabs' && (
            <div className="rounded-lg border border-sage-100 bg-sage-50/60 p-4">
              <div className="text-sm font-medium text-sage-800 mb-2">ElevenLabs delivery defaults</div>
              <div className="grid grid-cols-2 md:grid-cols-4 gap-3 text-xs">
                <div>
                  <div className="text-sage-400 uppercase tracking-wide">Model</div>
                  <div className="text-sage-700 font-mono mt-1">eleven_v3</div>
                </div>
                <div>
                  <div className="text-sage-400 uppercase tracking-wide">Stability</div>
                  <div className="text-sage-700 mt-1">65%</div>
                </div>
                <div>
                  <div className="text-sage-400 uppercase tracking-wide">Style</div>
                  <div className="text-sage-700 mt-1">0%</div>
                </div>
                <div>
                  <div className="text-sage-400 uppercase tracking-wide">Output</div>
                  <div className="text-sage-700 mt-1">MP3</div>
                </div>
              </div>
              <p className="text-xs text-sage-500 mt-3 leading-relaxed">
                Tuned for technical and financial narration: rational, stable, and low-flair so market content does not sound hyped or sarcastic.
              </p>
            </div>
          )}

          <div className="rounded-lg border border-sage-100 bg-white p-4">
            <div className="text-sm font-medium text-sage-800 mb-1">Audio output filenames</div>
            <p className="text-xs text-sage-500 leading-relaxed">
              Generated audio files use the episode title as a descriptive kebab-case filename, with the episode ID appended for uniqueness.
            </p>
            <div className="mt-3 rounded-md bg-sage-50 px-3 py-2 font-mono text-xs text-sage-700">
              q4-market-analysis-ebitda-qe-42.mp3
            </div>
          </div>

          {/* Save button */}
          <button
            onClick={async () => {
              setSavingSettings(true)
              try {
                const saved = await updateSettings(appSettings)
                setAppSettings(saved)
              } catch {
                alert('Failed to save settings')
              } finally {
                setSavingSettings(false)
              }
            }}
            disabled={savingSettings}
            className="px-4 py-2 bg-sage-600 text-white rounded-lg text-sm hover:bg-sage-700 disabled:opacity-50 transition-colors"
          >
            {savingSettings ? 'Saving...' : 'Save TTS Settings'}
          </button>
        </div>
      </section>

      {/* Services Section */}
      <section>
        <h2 className="text-xs font-semibold text-sage-400 uppercase tracking-widest mb-5">Services</h2>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {serviceEntries.map(([name, status]) => (
            <ServiceCard key={name} name={name} status={status} />
          ))}
        </div>
      </section>

      {/* API Usage Section */}
      <section>
        <h2 className="text-xs font-semibold text-sage-400 uppercase tracking-widest mb-5">API Usage</h2>
        {usage.length === 0 ? (
          <div className="rounded-xl border border-sage-100 bg-white p-5">
            <p className="text-sm text-sage-500 font-light">No API usage recorded yet.</p>
          </div>
        ) : (
          <div className="space-y-3">
            {usage.map((s) => {
              const maxCost = Math.max(...usage.map((u) => u.total_cost_usd))
              const barWidth = maxCost > 0 ? (s.total_cost_usd / maxCost) * 100 : 0
              return (
                <div key={s.service} className="rounded-xl border border-sage-100 bg-white p-5">
                  <div className="flex items-center justify-between mb-2">
                    <h3 className="text-sm font-medium text-sage-800 capitalize">{s.service}</h3>
                    <span className="text-sm font-medium text-sage-700">${s.total_cost_usd.toFixed(4)}</span>
                  </div>
                  <div className="w-full bg-sage-50 rounded-full h-2 mb-3">
                    <div
                      className="bg-sage-500 h-2 rounded-full transition-all"
                      style={{ width: `${Math.max(barWidth, 1)}%` }}
                    />
                  </div>
                  <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-sage-600">
                    <span><span className="text-sage-400">Requests:</span> {s.request_count.toLocaleString()}</span>
                    <span><span className="text-sage-400">Input tokens:</span> {s.total_input_tokens.toLocaleString()}</span>
                    <span><span className="text-sage-400">Output tokens:</span> {s.total_output_tokens.toLocaleString()}</span>
                  </div>
                </div>
              )
            })}
            <div className="text-right text-xs text-sage-500 font-light">
              Total: ${usage.reduce((sum, s) => sum + s.total_cost_usd, 0).toFixed(4)}
            </div>
          </div>
        )}
      </section>

      {/* Server Configuration Section */}
      <section>
        <h2 className="text-xs font-semibold text-sage-400 uppercase tracking-widest mb-5">Server Configuration</h2>
        <div className="rounded-xl border border-sage-100 bg-white p-5">
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 text-sm">
            <div>
              <span className="text-sage-400 text-xs">Port</span>
              <p className="text-sage-700 font-light">{data.server.port}</p>
            </div>
            <div>
              <span className="text-sage-400 text-xs">Database</span>
              <p className="text-sage-700 font-light">{data.server.db_path}</p>
            </div>
            <div>
              <span className="text-sage-400 text-xs">Authentication</span>
              <p className="text-sage-700 font-light">{data.server.auth_enabled ? 'Enabled' : 'Disabled'}</p>
            </div>
            <div>
              <span className="text-sage-400 text-xs">Web Directory</span>
              <p className="text-sage-700 font-light">{data.server.web_dir}</p>
            </div>
          </div>
        </div>
      </section>

      {/* Data Export Section */}
      <section>
        <h2 className="text-xs font-semibold text-sage-400 uppercase tracking-widest mb-5">Data Export</h2>
        <div className="rounded-xl border border-sage-100 bg-white p-5">
          <p className="text-sm text-sage-600 mb-4">Export all your notes for backup or migration.</p>
          <div className="flex gap-3">
            <a href="/api/export?format=json" download className="px-4 py-2 bg-sage-600 text-white rounded-lg text-sm hover:bg-sage-700 transition-colors">
              Export JSON
            </a>
            <a href="/api/export?format=markdown" download className="px-4 py-2 bg-sage-100 text-sage-700 rounded-lg text-sm hover:bg-sage-200 transition-colors">
              Export Markdown (ZIP)
            </a>
          </div>
        </div>
      </section>
    </div>
  )
}
