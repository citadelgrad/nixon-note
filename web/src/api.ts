export interface Note {
  id: number
  content: string
  content_type: string
  source_type: string
  source_url: string | null
  title: string | null
  summary: string | null
  created_at: string
  updated_at: string
  tags: string[]
}

export interface NotesResponse {
  notes: Note[]
  query: string | null
  count: number
  limit: number
  offset: number
}

export interface TagWithCount {
  id: number
  name: string
  count: number
}

export interface TagsResponse {
  tags: TagWithCount[]
}

let token = localStorage.getItem('note_token') ?? ''
let authErrorCallback: (() => void) | null = null

export function getToken(): string {
  return token
}

export function setToken(t: string) {
  token = t
  if (t) {
    localStorage.setItem('note_token', t)
  } else {
    localStorage.removeItem('note_token')
  }
}

export function onAuthError(cb: (() => void) | null) {
  authErrorCallback = cb
}

async function apiFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...(init?.headers as Record<string, string>),
  }
  if (token) {
    headers['Authorization'] = `Bearer ${token}`
  }
  const res = await fetch(path, { ...init, headers })
  if (!res.ok) {
    if (res.status === 401 && authErrorCallback) {
      authErrorCallback()
    }
    const body = await res.text().catch(() => '')
    throw new Error(body || `API error: ${res.status}`)
  }
  const text = await res.text()
  if (!text) return null as T
  return JSON.parse(text)
}

export function fetchNotes(query?: string, limit = 20, offset = 0, excludeTag?: string): Promise<NotesResponse> {
  const params = new URLSearchParams()
  if (query) params.set('q', query)
  params.set('limit', String(limit))
  params.set('offset', String(offset))
  if (excludeTag) params.set('exclude_tag', excludeTag)
  return apiFetch(`/api/notes?${params}`)
}

export function fetchNote(id: number): Promise<Note> {
  return apiFetch(`/api/notes/${id}`)
}

export function createNote(content: string): Promise<Note> {
  return apiFetch('/api/notes', {
    method: 'POST',
    body: JSON.stringify({ content, source_type: 'web' }),
  })
}

export function fetchTags(): Promise<TagsResponse> {
  return apiFetch('/api/tags')
}

export function fetchNotesByTag(tag: string, limit = 20): Promise<NotesResponse> {
  const params = new URLSearchParams()
  params.set('tag', tag)
  params.set('limit', String(limit))
  return apiFetch(`/api/tags/filter?${params}`)
}

export function updateNote(id: number, content: string): Promise<Note> {
  return apiFetch(`/api/notes/${id}`, {
    method: 'PUT',
    body: JSON.stringify({ content }),
  })
}

export interface UrlIngestResponse {
  note_id: number
  title: string
  url: string
  word_count: number
}

export function ingestUrl(url: string, tags?: string[]): Promise<UrlIngestResponse> {
  return apiFetch('/api/ingest/url', {
    method: 'POST',
    body: JSON.stringify({ url, tags }),
  })
}

export function fetchRandomNotes(limit = 3, excludeTag?: string): Promise<NotesResponse> {
  const params = new URLSearchParams()
  params.set('limit', String(limit))
  if (excludeTag) params.set('exclude_tag', excludeTag)
  return apiFetch(`/api/notes/random?${params}`)
}

export interface ServiceStatus {
  configured: boolean
  healthy: boolean
  url?: string
  model?: string
  model_size?: string
  model_dir?: string
}

export interface ServerConfig {
  port: string
  db_path: string
  auth_enabled: boolean
  web_dir: string
}

export interface StatusResponse {
  services: Record<string, ServiceStatus>
  server: ServerConfig
}

export function fetchStatus(): Promise<StatusResponse> {
  return apiFetch('/api/status')
}

export interface UsageSummary {
  service: string
  total_input_tokens: number
  total_output_tokens: number
  total_cost_usd: number
  request_count: number
}

export function fetchUsageSummary(): Promise<{ summary: UsageSummary[] }> {
  return apiFetch('/api/usage/summary')
}

export interface YouTubeIngestResponse {
  note_id: number
  title: string
  summary: string
}

export function ingestYoutube(url: string): Promise<YouTubeIngestResponse> {
  return apiFetch('/api/ingest/youtube', {
    method: 'POST',
    body: JSON.stringify({ url }),
  })
}

export async function deleteNote(id: number): Promise<void> {
  await apiFetch<null>(`/api/notes/${id}`, { method: 'DELETE' })
}

// --- Audio / Podcast ---

export interface AudioEpisode {
  id: number
  title: string
  episode_type: string
  content_mode: string
  tts_provider: string
  tts_voice: string
  status: string
  error_message: string | null
  audio_path: string | null
  file_size_bytes: number | null
  duration_seconds: number | null
  note_ids: number[]
  created_at: string
  updated_at: string
}

export interface GenerateAudioRequest {
  note_ids: number[]
  episode_type?: string
  content_mode?: string
  title?: string
}

export interface GenerateAudioResponse {
  episode_id: number
  status: string
}

export function generateAudio(req: GenerateAudioRequest): Promise<GenerateAudioResponse> {
  return apiFetch('/api/audio/generate', {
    method: 'POST',
    body: JSON.stringify(req),
  })
}

export function fetchEpisodes(): Promise<{ episodes: AudioEpisode[] }> {
  return apiFetch('/api/audio')
}

export function fetchEpisode(id: number): Promise<AudioEpisode> {
  return apiFetch(`/api/audio/${id}`)
}

export async function deleteEpisode(id: number): Promise<void> {
  await apiFetch<null>(`/api/audio/${id}`, { method: 'DELETE' })
}

export function audioFileUrl(id: number): string {
  return `/api/audio/${id}/file`
}

// --- Settings ---

export type AppSettings = Record<string, string>

interface SettingsResponse {
  settings: AppSettings
}

export async function fetchSettings(): Promise<AppSettings> {
  const res = await apiFetch<SettingsResponse>('/api/settings')
  return res.settings
}

export async function updateSettings(settings: AppSettings): Promise<AppSettings> {
  const res = await apiFetch<SettingsResponse>('/api/settings', {
    method: 'PUT',
    body: JSON.stringify(settings),
  })
  return res.settings
}
