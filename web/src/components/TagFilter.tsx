import { useEffect, useState } from 'react'
import type { TagWithCount } from '../api'
import { fetchTags } from '../api'

interface TagFilterProps {
  selectedTag: string | null
  onSelectTag: (tag: string | null) => void
}

export function TagFilter({ selectedTag, onSelectTag }: TagFilterProps) {
  const [tags, setTags] = useState<TagWithCount[]>([])
  const [loading, setLoading] = useState(false)

  useEffect(() => {
    async function loadTags() {
      setLoading(true)
      try {
        const res = await fetchTags()
        setTags(res.tags)
      } catch (e) {
        console.error('Failed to load tags:', e)
      } finally {
        setLoading(false)
      }
    }
    loadTags()
  }, [])

  if (loading) {
    return <div className="text-sage-400 text-sm">Loading tags...</div>
  }

  const activeTags = tags.filter(tag => tag.count > 0)

  if (activeTags.length === 0) {
    return <div className="text-sage-400 text-sm italic">No tags yet</div>
  }

  return (
    <div className="space-y-2">
      <button
        onClick={() => onSelectTag(null)}
        className={`w-full text-left px-3 py-1.5 rounded-lg transition-colors ${
          selectedTag === null
            ? 'bg-sage-100 text-sage-700 font-medium'
            : 'text-sage-600 hover:bg-sage-50'
        }`}
      >
        All notes
      </button>
      {activeTags.map((tag) => (
        <button
          key={tag.id}
          onClick={() => onSelectTag(tag.name)}
          className={`w-full text-left px-3 py-1.5 rounded-lg transition-colors flex items-center justify-between ${
            selectedTag === tag.name
              ? 'bg-sage-100 text-sage-700 font-medium'
              : 'text-sage-600 hover:bg-sage-50'
          }`}
        >
          <span>{tag.name}</span>
          <span className="text-xs text-sage-400">{tag.count}</span>
        </button>
      ))}
    </div>
  )
}
