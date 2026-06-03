import { useEffect, useRef } from 'react'

interface SearchBarProps {
  value: string
  onChange: (value: string) => void
}

export function SearchBar({ value, onChange }: SearchBarProps) {
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    inputRef.current?.focus()
  }, [])

  return (
    <div className="relative group">
      <div className="absolute left-5 top-1/2 -translate-y-1/2 text-sage-400 transition-colors group-focus-within:text-sage-600">
        <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
        </svg>
      </div>
      <input
        ref={inputRef}
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder="Search your notes..."
        className="w-full rounded-full border border-sage-200/60 bg-white/80 backdrop-blur-sm pl-14 pr-12 py-4 text-base font-light
                   placeholder:text-sage-300 focus:border-sage-400/60 focus:outline-none
                   focus:ring-2 focus:ring-sage-400/20 focus:bg-white transition-all shadow-sm"
      />
      {value && (
        <button
          onClick={() => onChange('')}
          className="absolute right-5 top-1/2 -translate-y-1/2 w-6 h-6 flex items-center justify-center rounded-full
                     bg-sage-100 text-sage-500 hover:bg-sage-200 hover:text-sage-700 transition-all"
          aria-label="Clear search"
        >
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      )}
    </div>
  )
}
