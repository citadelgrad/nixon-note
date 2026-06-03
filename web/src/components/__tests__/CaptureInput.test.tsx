import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { CaptureInput } from '../CaptureInput'

const DRAFT_KEY = 'nixonnote:capture-draft'

describe('CaptureInput', () => {
  const mockOnCapture = vi.fn()

  beforeEach(() => {
    vi.clearAllMocks()
    localStorage.clear()
  })

  describe('modal backdrop behavior', () => {
    it('does not close modal on backdrop click when textarea has content', async () => {
      const user = userEvent.setup()
      render(<CaptureInput onCapture={mockOnCapture} />)

      // Open modal via Cmd+K
      await user.keyboard('{Meta>}k{/Meta}')
      expect(screen.getByText('Capture Thought')).toBeInTheDocument()

      // Type some text
      const textarea = screen.getByPlaceholderText('Capture an idea, task, or note...')
      await user.type(textarea, 'my important thought')

      // Click the backdrop (the outer overlay div)
      const backdrop = screen.getByText('Capture Thought').closest('.fixed')!
      await user.click(backdrop)

      // Modal should still be open because there's text content
      expect(screen.getByText('Capture Thought')).toBeInTheDocument()
      expect(screen.getByDisplayValue('my important thought')).toBeInTheDocument()
    })

    it('closes modal on backdrop click when textarea is empty', async () => {
      const user = userEvent.setup()
      render(<CaptureInput onCapture={mockOnCapture} />)

      // Open modal via Cmd+K
      await user.keyboard('{Meta>}k{/Meta}')
      expect(screen.getByText('Capture Thought')).toBeInTheDocument()

      // Click the backdrop without typing anything
      const backdrop = screen.getByText('Capture Thought').closest('.fixed')!
      await user.click(backdrop)

      // Modal should close since textarea is empty
      expect(screen.queryByText('Capture Thought')).not.toBeInTheDocument()
    })

    it('closes modal on Escape even when textarea has content', async () => {
      const user = userEvent.setup()
      render(<CaptureInput onCapture={mockOnCapture} />)

      // Open modal and type
      await user.keyboard('{Meta>}k{/Meta}')
      const textarea = screen.getByPlaceholderText('Capture an idea, task, or note...')
      await user.type(textarea, 'some text')

      // Escape should still close (intentional close)
      await user.keyboard('{Escape}')
      expect(screen.queryByText('Capture Thought')).not.toBeInTheDocument()
    })
  })

  describe('draft auto-save and restore', () => {
    it('saves draft to localStorage on text input', async () => {
      const user = userEvent.setup()
      render(<CaptureInput onCapture={mockOnCapture} />)

      await user.keyboard('{Meta>}k{/Meta}')
      const textarea = screen.getByPlaceholderText('Capture an idea, task, or note...')
      await user.type(textarea, 'draft content')

      expect(localStorage.getItem(DRAFT_KEY)).toBe('draft content')
    })

    it('restores draft when modal is reopened', async () => {
      const user = userEvent.setup()
      render(<CaptureInput onCapture={mockOnCapture} />)

      // Open, type, close via Escape
      await user.keyboard('{Meta>}k{/Meta}')
      const textarea = screen.getByPlaceholderText('Capture an idea, task, or note...')
      await user.type(textarea, 'saved draft')
      await user.keyboard('{Escape}')

      // Reopen
      await user.keyboard('{Meta>}k{/Meta}')
      expect(screen.getByDisplayValue('saved draft')).toBeInTheDocument()
    })

    it('clears draft after successful save', async () => {
      const user = userEvent.setup()
      render(<CaptureInput onCapture={mockOnCapture} />)

      await user.keyboard('{Meta>}k{/Meta}')
      const textarea = screen.getByPlaceholderText('Capture an idea, task, or note...')
      await user.type(textarea, 'note to save')

      // Submit with Cmd+Enter
      await user.keyboard('{Meta>}{Enter}{/Meta}')

      expect(mockOnCapture).toHaveBeenCalledWith('note to save')
      expect(localStorage.getItem(DRAFT_KEY)).toBeNull()
    })

    it('preserves draft across close and reopen cycles', async () => {
      const user = userEvent.setup()
      render(<CaptureInput onCapture={mockOnCapture} />)

      // Open, type, close
      await user.keyboard('{Meta>}k{/Meta}')
      const textarea = screen.getByPlaceholderText('Capture an idea, task, or note...')
      await user.type(textarea, 'persistent draft')
      await user.keyboard('{Escape}')

      // Reopen - draft should be there
      await user.keyboard('{Meta>}k{/Meta}')
      expect(screen.getByDisplayValue('persistent draft')).toBeInTheDocument()

      // Close again
      await user.keyboard('{Escape}')

      // Reopen again - still there
      await user.keyboard('{Meta>}k{/Meta}')
      expect(screen.getByDisplayValue('persistent draft')).toBeInTheDocument()
    })
  })

  describe('modal submit', () => {
    it('submits via Save Note button', async () => {
      const user = userEvent.setup()
      render(<CaptureInput onCapture={mockOnCapture} />)

      await user.keyboard('{Meta>}k{/Meta}')
      const textarea = screen.getByPlaceholderText('Capture an idea, task, or note...')
      await user.type(textarea, 'button submit')

      await user.click(screen.getByText('Save Note'))
      expect(mockOnCapture).toHaveBeenCalledWith('button submit')
    })

    it('disables Save Note button when textarea is empty', async () => {
      const user = userEvent.setup()
      render(<CaptureInput onCapture={mockOnCapture} />)

      await user.keyboard('{Meta>}k{/Meta}')
      const saveButton = screen.getByText('Save Note')
      expect(saveButton).toBeDisabled()
    })
  })
})
