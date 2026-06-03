import type { Note } from '../api'
import { NoteCard } from './NoteCard'

interface DailyDigestProps {
  notes: Note[]
  onNoteDeleted?: () => void
  onTagClick?: (tag: string) => void
  onNoteExpand?: (note: Note) => void
  onAudioGenerated?: (episodeId: number) => void
}

interface NotesGroupedByDay {
  date: string
  dateLabel: string
  notes: Note[]
}

export function DailyDigest({ notes, onNoteDeleted, onTagClick, onNoteExpand, onAudioGenerated }: DailyDigestProps) {
  const grouped = groupNotesByDay(notes)

  if (grouped.length === 0) {
    return (
      <div className="rounded-xl bg-sage-50 p-8 text-center">
        <p className="text-sage-400">
          No notes yet. Capture your first thought using the input below or the voice button.
        </p>
      </div>
    )
  }

  return (
    <div className="space-y-8">
      {grouped.map((group) => (
        <div key={group.date}>
          <div className="sticky top-0 bg-white/90 backdrop-blur-sm py-2 mb-4 border-b border-sage-200">
            <h2 className="text-sm font-semibold text-sage-700 uppercase tracking-wide">
              {group.dateLabel}
            </h2>
          </div>

          <div className="space-y-4">
            {group.notes.map((note) => (
              <NoteCard
                key={note.id}
                note={note}
                onDelete={onNoteDeleted}
                onTagClick={onTagClick}
                onExpand={onNoteExpand}
                onAudioGenerated={onAudioGenerated}
              />
            ))}
          </div>
        </div>
      ))}
    </div>
  )
}

function groupNotesByDay(notes: Note[]): NotesGroupedByDay[] {
  const groups = new Map<string, Note[]>()

  for (const note of notes) {
    const date = new Date(note.created_at)
    const dateKey = formatDateKey(date)

    if (!groups.has(dateKey)) {
      groups.set(dateKey, [])
    }
    groups.get(dateKey)!.push(note)
  }

  // Convert to array and sort by date (newest first)
  const result: NotesGroupedByDay[] = []

  for (const [dateKey, notesForDay] of groups) {
    const date = new Date(dateKey)
    result.push({
      date: dateKey,
      dateLabel: formatDateLabel(date),
      notes: notesForDay,
    })
  }

  result.sort((a, b) => new Date(b.date).getTime() - new Date(a.date).getTime())

  return result
}

function formatDateKey(date: Date): string {
  // Return YYYY-MM-DD for grouping
  return date.toISOString().split('T')[0]
}

function formatDateLabel(date: Date): string {
  const today = new Date()
  const yesterday = new Date(today)
  yesterday.setDate(yesterday.getDate() - 1)

  const dateKey = formatDateKey(date)
  const todayKey = formatDateKey(today)
  const yesterdayKey = formatDateKey(yesterday)

  if (dateKey === todayKey) {
    return 'Today'
  } else if (dateKey === yesterdayKey) {
    return 'Yesterday'
  } else {
    // Format as "Monday, Feb 4" or "Monday, Feb 4, 2025" for older years
    const options: Intl.DateTimeFormatOptions = {
      weekday: 'long',
      month: 'short',
      day: 'numeric',
    }

    if (date.getFullYear() !== today.getFullYear()) {
      options.year = 'numeric'
    }

    return date.toLocaleDateString('en-US', options)
  }
}
