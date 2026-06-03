import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ChatInterface } from '../ChatInterface'

function streamResponse(chunks: string[]): Response {
  const encoder = new TextEncoder()
  return new Response(new ReadableStream({
    start(controller) {
      for (const chunk of chunks) {
        controller.enqueue(encoder.encode(chunk))
      }
      controller.close()
    },
  }), { status: 200 })
}

describe('ChatInterface', () => {
  beforeEach(() => {
    localStorage.clear()
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('renders the empty state and disabled send button', () => {
    render(<ChatInterface />)

    expect(screen.getByText('Ask me anything about your notes')).toBeInTheDocument()
    expect(screen.getByText(/I can search your knowledge base/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled()
  })

  it('posts the user message with the auth token and renders streamed assistant text', async () => {
    const fetchMock = vi.mocked(fetch)
    fetchMock.mockResolvedValueOnce(streamResponse([
      'data: {"type":"text-delta","delta":"Hello"}\n',
      'data: {"type":"text-delta","delta":" there"}\n',
    ]))
    localStorage.setItem('note_token', 'test-token-123')
    const user = userEvent.setup()

    render(<ChatInterface />)

    await user.type(screen.getByPlaceholderText(/Ask a question about your notes/i), 'test message')
    await user.click(screen.getByRole('button', { name: 'Send' }))

    expect(fetchMock).toHaveBeenCalledWith('/api/chat/stream', expect.objectContaining({
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': 'Bearer test-token-123',
      },
      body: JSON.stringify({ message: 'test message', max_results: 5 }),
    }))
    expect(screen.getByText('test message')).toBeInTheDocument()
    await waitFor(() => {
      expect(screen.getByText('Hello there')).toBeInTheDocument()
    })
  })

  it('shows a stop button while a stream is pending', async () => {
    const fetchMock = vi.mocked(fetch)
    fetchMock.mockReturnValueOnce(new Promise<Response>(() => {}))
    const user = userEvent.setup()

    render(<ChatInterface />)

    await user.type(screen.getByPlaceholderText(/Ask a question about your notes/i), 'slow question')
    await user.click(screen.getByRole('button', { name: 'Send' }))

    expect(await screen.findByRole('button', { name: 'Stop' })).toBeInTheDocument()
  })

  it('displays request errors and disables input until retry', async () => {
    const fetchMock = vi.mocked(fetch)
    fetchMock.mockResolvedValueOnce(new Response('GEMINI_API_KEY environment variable is not set', { status: 500 }))
    const user = userEvent.setup()

    render(<ChatInterface />)

    await user.type(screen.getByPlaceholderText(/Ask a question about your notes/i), 'break please')
    await user.click(screen.getByRole('button', { name: 'Send' }))

    expect(await screen.findByText('Unable to generate response')).toBeInTheDocument()
    expect(screen.getByText(/GEMINI_API_KEY environment variable is not set/)).toBeInTheDocument()
    expect(screen.getByPlaceholderText(/Fix the error above to continue/)).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Retry' })).toBeInTheDocument()
  })

  it('renders markdown and clickable note references in assistant messages', async () => {
    const fetchMock = vi.mocked(fetch)
    fetchMock.mockResolvedValueOnce(streamResponse([
      'data: {"type":"text-delta","delta":"**Bold** and [Note 42]"}\n',
    ]))
    const user = userEvent.setup()

    render(<ChatInterface />)

    await user.type(screen.getByPlaceholderText(/Ask a question about your notes/i), 'markdown please')
    await user.click(screen.getByRole('button', { name: 'Send' }))

    await waitFor(() => {
      expect(screen.getByText('Bold')).toBeInTheDocument()
    })
    expect(screen.getByRole('button', { name: /Note 42/ })).toBeInTheDocument()
  })
})
