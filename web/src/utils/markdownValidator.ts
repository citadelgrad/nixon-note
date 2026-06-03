export interface MarkdownValidationIssue {
  line: number
  column: number
  message: string
  severity: 'error' | 'warning'
}

export function validateMarkdown(content: string): MarkdownValidationIssue[] {
  const issues: MarkdownValidationIssue[] = []
  const lines = content.split('\n')

  lines.forEach((line, index) => {
    const lineNumber = index + 1

    // Check for malformed headers (missing space after #)
    if (/^#{1,6}[^#\s]/.test(line)) {
      issues.push({
        line: lineNumber,
        column: line.indexOf('#') + (line.match(/^#+/)?.[0].length ?? 0) + 1,
        message: 'Headers should have a space after the # symbols (e.g., "## Header" not "##Header")',
        severity: 'error',
      })
    }

    // Check for unbalanced code backticks (inline code)
    const backtickCount = (line.match(/`/g) || []).length
    if (backtickCount % 2 !== 0 && !line.trim().startsWith('```')) {
      issues.push({
        line: lineNumber,
        column: line.lastIndexOf('`') + 1,
        message: 'Unmatched backtick - inline code should be wrapped in pairs of backticks',
        severity: 'warning',
      })
    }

    // Check for broken list formatting (list items without space after marker)
    if (/^(\*|-|\+|\d+\.)[^\s]/.test(line.trim())) {
      issues.push({
        line: lineNumber,
        column: line.indexOf(line.trim()) + 1,
        message: 'List items should have a space after the marker (e.g., "- item" not "-item")',
        severity: 'error',
      })
    }

    // Check for unbalanced bold markers
    const boldCount = (line.match(/\*\*/g) || []).length
    if (boldCount % 2 !== 0) {
      issues.push({
        line: lineNumber,
        column: line.lastIndexOf('**') + 1,
        message: 'Unmatched ** - bold text should be wrapped in pairs of **',
        severity: 'warning',
      })
    }

    // Check for unbalanced italic markers (single *)
    // This is trickier because * is also used for lists
    const italicMatches = line.match(/(?<!\*)\*(?!\*)(?=[^\s])/g)
    if (italicMatches && italicMatches.length % 2 !== 0 && !line.trim().startsWith('*')) {
      issues.push({
        line: lineNumber,
        column: line.lastIndexOf('*') + 1,
        message: 'Unmatched * - italic text should be wrapped in pairs of *',
        severity: 'warning',
      })
    }
  })

  // Check for unmatched code fences
  const codeFenceCount = lines.filter((line: string) => line.trim().startsWith('```')).length
  if (codeFenceCount % 2 !== 0) {
    // Find last code fence index manually (backwards compatibility)
    let lastFenceLine = -1
    for (let i = lines.length - 1; i >= 0; i--) {
      if (lines[i].trim().startsWith('```')) {
        lastFenceLine = i
        break
      }
    }
    issues.push({
      line: lastFenceLine + 1,
      column: 1,
      message: 'Unmatched code fence - code blocks should have opening and closing ``` markers',
      severity: 'error',
    })
  }

  // Check for broken link syntax
  lines.forEach((line, index) => {
    const lineNumber = index + 1

    // Find potential broken markdown links [text](url)
    const brokenLinkPattern = /\[([^\]]*)\]\s*(?!\()/g
    let match
    while ((match = brokenLinkPattern.exec(line)) !== null) {
      // Skip if it's a reference-style link or checkbox
      if (!/^\[([^\]]*)\]:\s*http/.test(line) && !/^\s*-\s*\[/.test(line)) {
        issues.push({
          line: lineNumber,
          column: match.index + 1,
          message: 'Possible broken link - [text] should be followed by (url) for inline links',
          severity: 'warning',
        })
      }
    }
  })

  return issues.sort((a, b) => a.line - b.line)
}
