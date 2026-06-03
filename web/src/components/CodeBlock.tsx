import { useState } from 'react'

interface CodeBlockProps {
  children: string
  className?: string
}

export function CodeBlock({ children, className }: CodeBlockProps) {
  const [copied, setCopied] = useState(false)

  const handleCopy = async () => {
    await navigator.clipboard.writeText(children)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  return (
    <div className="relative group">
      <pre className={className}>
        <code>{children}</code>
      </pre>
      <button
        onClick={handleCopy}
        className="absolute top-2 right-2 px-2 py-1 bg-sage-600 text-white text-xs rounded opacity-0 group-hover:opacity-100 transition-opacity hover:bg-sage-700"
        title="Copy code"
      >
        {copied ? '✓ Copied!' : 'Copy'}
      </button>
    </div>
  )
}
