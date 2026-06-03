import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Settings } from '../Settings'
import { updateSettings } from '../../api'

vi.mock('../../api', async () => {
  const actual = await vi.importActual<typeof import('../../api')>('../../api')
  return {
    ...actual,
    fetchStatus: vi.fn().mockResolvedValue({
      services: {
        elevenlabs_tts: { configured: true, healthy: true },
      },
      server: { port: '9999', db_path: '/tmp/nixonnote.db', auth_enabled: false, web_dir: 'web/dist' },
    }),
    fetchUsageSummary: vi.fn().mockResolvedValue({ summary: [] }),
    fetchSettings: vi.fn().mockResolvedValue({
      tts_provider: 'elevenlabs',
      tts_voice_elevenlabs: 'existingVoiceId',
      tts_voice_elevenlabs_custom_name: 'Existing Voice',
      tts_voice_elevenlabs_custom_id: 'existingVoiceId',
    }),
    updateSettings: vi.fn().mockResolvedValue({}),
  }
})

describe('Settings', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('lets the user enter and save a custom ElevenLabs voice name and id', async () => {
    const user = userEvent.setup()
    vi.mocked(updateSettings).mockImplementation(async (settings) => settings)

    render(<Settings />)

    const nameInput = await screen.findByLabelText('Custom ElevenLabs voice name')
    const idInput = screen.getByLabelText('Custom ElevenLabs voice ID')

    await user.clear(nameInput)
    await user.type(nameInput, 'Scott Custom Narrator')
    await user.clear(idInput)
    await user.type(idInput, 'abc123CustomVoice')

    expect(screen.getByRole('button', { name: /Use custom ElevenLabs voice/i }).className).toContain('border-sage-500')
    expect(screen.getAllByRole('link', { name: 'Preview in ElevenLabs ↗' })[0]).toHaveAttribute(
      'href',
      'https://elevenlabs.io/app/voice-library?voiceId=abc123CustomVoice',
    )

    await user.click(screen.getByRole('button', { name: 'Save TTS Settings' }))

    await waitFor(() => {
      expect(updateSettings).toHaveBeenCalledWith(expect.objectContaining({
        tts_provider: 'elevenlabs',
        tts_voice_elevenlabs: 'abc123CustomVoice',
        tts_voice_elevenlabs_custom_name: 'Scott Custom Narrator',
        tts_voice_elevenlabs_custom_id: 'abc123CustomVoice',
      }))
    })
  })

  it('renders built-in ElevenLabs voice ids as copy buttons and direct preview links', async () => {
    render(<Settings />)

    const copyButton = await screen.findByRole('button', { name: 'Copy Nicole voice ID' })

    expect(copyButton.textContent).toBe('piTKgcLEGmPE4e6mEKli')
    expect(screen.getAllByRole('link', { name: 'Preview in ElevenLabs ↗' })[2]).toHaveAttribute(
      'href',
      'https://elevenlabs.io/app/voice-library?voiceId=piTKgcLEGmPE4e6mEKli',
    )
  })
})
