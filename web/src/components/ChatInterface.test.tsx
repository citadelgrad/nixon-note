import { describe, it, expect } from 'vitest'

// Test the preprocessing function directly
function preprocessNoteReferences(content: string): string {
  let processed = content
  // Pattern 1: [Note 123] format
  processed = processed.replace(/\[Note (\d+)\](?!\()/gi, '[📝 Note $1](#note-$1)')
  // Pattern 2: [123] format (pure numbers)
  processed = processed.replace(/\[(\d+)\](?!\()/g, '[📝 $1](#note-$1)')
  return processed
}

describe('ChatInterface - Note Reference Preprocessing', () => {
  it('should convert simple note reference', () => {
    const input = 'Check out note [317] for details.'
    const expected = 'Check out note [📝 317](#note-317) for details.'
    expect(preprocessNoteReferences(input)).toBe(expected)
  })

  it('should convert multiple note references', () => {
    const input = 'See [317] and [236] for more info.'
    const expected = 'See [📝 317](#note-317) and [📝 236](#note-236) for more info.'
    expect(preprocessNoteReferences(input)).toBe(expected)
  })

  it('should not convert markdown links', () => {
    const input = 'This is a [link](http://example.com) not a note.'
    const expected = 'This is a [link](http://example.com) not a note.'
    expect(preprocessNoteReferences(input)).toBe(expected)
  })

  it('should handle note references at start and end', () => {
    const input = '[317] is important and also [236]'
    const expected = '[📝 317](#note-317) is important and also [📝 236](#note-236)'
    expect(preprocessNoteReferences(input)).toBe(expected)
  })

  it('should handle complex markdown with note references', () => {
    const input = '**Bold text** with [317] and *italic* [236].'
    const expected = '**Bold text** with [📝 317](#note-317) and *italic* [📝 236](#note-236).'
    expect(preprocessNoteReferences(input)).toBe(expected)
  })

  it('should handle note references in lists', () => {
    const input = '* Item with [317]\n* Another item [236]'
    const expected = '* Item with [📝 317](#note-317)\n* Another item [📝 236](#note-236)'
    expect(preprocessNoteReferences(input)).toBe(expected)
  })

  it('should convert [Note 3] format references', () => {
    const input = 'See [Note 3] for details.'
    const expected = 'See [📝 Note 3](#note-3) for details.'
    expect(preprocessNoteReferences(input)).toBe(expected)
  })

  it('should handle both [Note 3] and [3] formats together', () => {
    const input = 'Check [Note 3] and also [236] for information.'
    const expected = 'Check [📝 Note 3](#note-3) and also [📝 236](#note-236) for information.'
    expect(preprocessNoteReferences(input)).toBe(expected)
  })

  it('should be case insensitive for [Note 3] format', () => {
    const input = 'See [note 3] and [NOTE 5].'
    const expected = 'See [📝 Note 3](#note-3) and [📝 Note 5](#note-5).'
    expect(preprocessNoteReferences(input)).toBe(expected)
  })
})
