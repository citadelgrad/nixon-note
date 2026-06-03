import { useState, useRef, useEffect } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import rehypeHighlight from 'rehype-highlight'
import { NoteModal } from './NoteModal'
import 'highlight.js/styles/github.css'

interface Message {
  id: string
  role: 'user' | 'assistant'
  content: string
  timestamp: Date
}

// Pre-process content to convert note references to markdown links
function preprocessNoteReferences(content: string): string {
  // Convert [Note 123] or [123] to clickable links, but not [text](url) patterns
  let processed = content

  // Pattern 1: [Note 123] format
  processed = processed.replace(/\[Note (\d+)\](?!\()/gi, '[📝 Note $1](#note-$1)')

  // Pattern 2: [123] format (pure numbers)
  processed = processed.replace(/\[(\d+)\](?!\()/g, '[📝 $1](#note-$1)')

  return processed
}

export function ChatInterface() {
  const [input, setInput] = useState('')
  const [messages, setMessages] = useState<Message[]>([])
  const [isStreaming, setIsStreaming] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [selectedNoteId, setSelectedNoteId] = useState<number | null>(null)
  const messagesEndRef = useRef<HTMLDivElement>(null)
  const abortControllerRef = useRef<AbortController | null>(null)

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }

  useEffect(() => {
    scrollToBottom()
  }, [messages])

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (input.trim() && !isStreaming) {
      const userMessage: Message = {
        id: `user_${Date.now()}`,
        role: 'user',
        content: input.trim(),
        timestamp: new Date(),
      }

      setMessages(prev => [...prev, userMessage])
      setInput('')
      setError(null)
      setIsStreaming(true)

      const assistantMessage: Message = {
        id: `assistant_${Date.now()}`,
        role: 'assistant',
        content: '',
        timestamp: new Date(),
      }
      setMessages(prev => [...prev, assistantMessage])

      try {
        abortControllerRef.current = new AbortController()

        const response = await fetch('/api/chat/stream', {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            'Authorization': `Bearer ${localStorage.getItem('note_token')}`,
          },
          body: JSON.stringify({
            message: userMessage.content,
            max_results: 5,
          }),
          signal: abortControllerRef.current.signal,
        })

        if (!response.ok) {
          const body = await response.text().catch(() => '')
          throw new Error(body || `Chat request failed (${response.status})`)
        }

        const reader = response.body?.getReader()
        const decoder = new TextDecoder()

        if (!reader) {
          throw new Error('Response body is not readable')
        }

        while (true) {
          const { done, value } = await reader.read()
          if (done) break

          const chunk = decoder.decode(value, { stream: true })
          const lines = chunk.split('\n')

          for (const line of lines) {
            if (line.startsWith('data: ')) {
              const data = line.slice(6)
              try {
                const event = JSON.parse(data)
                if (event.type === 'text-delta' && event.delta) {
                  setMessages(prev => {
                    const updated = [...prev]
                    const lastMessage = updated[updated.length - 1]
                    if (lastMessage.role === 'assistant') {
                      lastMessage.content += event.delta
                    }
                    return updated
                  })
                }
              } catch (e) {
                console.warn('Failed to parse SSE line:', data, e)
              }
            }
          }
        }
      } catch (err) {
        if (err instanceof DOMException && err.name === 'AbortError') {
          // Stream was cancelled by user
        } else {
          const message = err instanceof Error ? err.message : 'An error occurred while streaming'
          console.error('Streaming error:', err)
          setError(message)
          // Remove the empty assistant message if there was an error
          setMessages(prev => prev.filter(m => m.id !== assistantMessage.id))
        }
      } finally {
        setIsStreaming(false)
        abortControllerRef.current = null
      }
    }
  }

  const handleStop = () => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort()
    }
  }

  const handleRetry = () => {
    setError(null)
    // Remove the last assistant message if it exists and is empty
    setMessages(prev => {
      const filtered = [...prev]
      if (filtered.length > 0 && filtered[filtered.length - 1].role === 'assistant' && !filtered[filtered.length - 1].content) {
        filtered.pop()
      }
      // Get the last user message and resend it
      const lastUserMessage = filtered.reverse().find(m => m.role === 'user')
      if (lastUserMessage) {
        setInput(lastUserMessage.content)
      }
      return filtered.reverse()
    })
  }

  const isDisabled = isStreaming || error != null

  return (
    <>
      <div className="flex flex-col h-full bg-cream-50 rounded-2xl border border-sage-100/60 shadow-sm overflow-hidden"
        style={{
          backgroundImage: `
            radial-gradient(circle at 20% 80%, rgba(139, 154, 120, 0.03) 0%, transparent 50%),
            radial-gradient(circle at 80% 20%, rgba(107, 125, 90, 0.04) 0%, transparent 50%)
          `
        }}
      >

      {/* Messages */}
      <div className="flex-1 overflow-y-auto px-8 py-8 space-y-8">
        {messages.length === 0 && (
          <div className="flex flex-col items-center justify-center h-full text-center px-6">
            <div className="w-20 h-20 mb-6 rounded-full bg-gradient-to-br from-sage-100 to-sage-50 flex items-center justify-center shadow-inner border border-sage-100/50">
              <svg className="w-10 h-10 text-sage-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-5 5v-5z" />
              </svg>
            </div>
            <p className="text-2xl font-light text-sage-700 mb-3 tracking-tight">Ask me anything about your notes</p>
            <p className="text-sm text-sage-400 font-light max-w-md leading-relaxed">
              I can search your knowledge base, synthesize information across notes, and help you rediscover what you've learned.
            </p>
          </div>
        )}

        {messages.map((message, idx) => (
          <div
            key={message.id}
            className={`flex ${message.role === 'user' ? 'justify-end' : 'justify-start'}`}
            style={{
              animation: 'fadeInUp 0.4s ease-out',
              animationDelay: `${idx * 0.05}s`,
              animationFillMode: 'both'
            }}
          >
            <div className={`flex flex-col gap-1 ${message.role === 'user' ? 'items-end' : 'items-start'}`}>
              <div
                className={`max-w-3xl px-6 py-4 rounded-2xl ${
                  message.role === 'user'
                    ? 'bg-sage-500 text-cream shadow-sm'
                    : 'bg-white border border-sage-100/60 text-sage-800 shadow-sm'
                }`}
              >
              {message.role === 'assistant' ? (
                <div className="prose prose-sage prose-sm max-w-none">
                  <ReactMarkdown
                    remarkPlugins={[remarkGfm]}
                    rehypePlugins={[rehypeHighlight]}
                    components={{
                      // Custom link handler for note references
                      a: ({ href, children, ...props }) => {
                        // Check if this is a note reference link
                        if (href?.startsWith('#note-')) {
                          const noteId = parseInt(href.slice(6))
                          return (
                            <button
                              onClick={(e) => {
                                e.preventDefault()
                                setSelectedNoteId(noteId)
                              }}
                              className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-sage-100 hover:bg-sage-200 text-sage-700 hover:text-sage-900 transition-colors font-medium text-xs no-underline cursor-pointer"
                              title={`View note ${noteId}`}
                            >
                              {children}
                            </button>
                          )
                        }
                        // Regular link
                        return <a href={href} {...props}>{children}</a>
                      }
                    }}
                  >
                    {preprocessNoteReferences(message.content)}
                  </ReactMarkdown>
                </div>
              ) : (
                <p className="whitespace-pre-wrap font-light">{message.content}</p>
              )}
              </div>
              <span className="text-xs text-sage-400 px-2">
                {message.timestamp.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' })}
              </span>
            </div>
          </div>
        ))}

        {error && (
          <div className="flex justify-start">
            <div className="max-w-3xl px-6 py-5 rounded-2xl bg-muted-red/5 border border-muted-red/20 shadow-sm">
              <div className="flex items-start gap-4">
                <div className="flex-shrink-0 w-10 h-10 rounded-full bg-muted-red/10 flex items-center justify-center">
                  <svg className="w-5 h-5 text-muted-red" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                  </svg>
                </div>
                <div className="flex-1">
                  <p className="text-sm font-semibold text-muted-red mb-2">Unable to generate response</p>
                  <p className="text-sm text-sage-600 font-light leading-relaxed mb-4">
                    {error || 'An unexpected error occurred. Please try again.'}
                  </p>
                  <button
                    onClick={handleRetry}
                    className="px-5 py-2.5 text-sm font-medium text-muted-red border border-muted-red/30 rounded-full hover:bg-muted-red/10 transition-all"
                  >
                    Retry
                  </button>
                </div>
              </div>
            </div>
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      {/* Input */}
      <form onSubmit={handleSubmit} className="px-8 py-6 border-t border-sage-100/60 bg-white/50 backdrop-blur-sm">
        <div className="flex gap-3">
          <input
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            disabled={isDisabled}
            placeholder={error ? "Fix the error above to continue" : "Ask a question about your notes..."}
            className="flex-1 px-5 py-3.5 bg-sage-50/50 border border-sage-200/60 rounded-full focus:outline-none focus:ring-2 focus:ring-sage-400/40 focus:border-sage-400/60 focus:bg-white disabled:opacity-50 disabled:cursor-not-allowed text-sage-800 placeholder-sage-400 font-light transition-all"
            autoFocus
          />
          {isStreaming ? (
            <button
              type="button"
              onClick={handleStop}
              className="px-7 py-3.5 bg-muted-red text-cream rounded-full font-medium hover:bg-opacity-90 transition-all shadow-sm"
            >
              Stop
            </button>
          ) : (
            <button
              type="submit"
              disabled={!input.trim() || isDisabled}
              className="px-7 py-3.5 bg-sage-500 text-cream rounded-full font-medium hover:bg-sage-600 transition-all disabled:opacity-40 disabled:cursor-not-allowed shadow-sm"
            >
              Send
            </button>
          )}
        </div>
      </form>
    </div>

    {/* Note Modal */}
    {selectedNoteId && (
      <NoteModal
        noteId={selectedNoteId}
        onClose={() => setSelectedNoteId(null)}
      />
    )}
  </>
  )
}
