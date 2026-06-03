import { useState, useRef, useEffect } from 'react'

interface VoiceRecorderProps {
  onComplete: () => void
  disabled?: boolean
}

function formatDuration(seconds: number): string {
  const mins = Math.floor(seconds / 60)
  const secs = seconds % 60
  return `${String(mins).padStart(2, '0')}:${String(secs).padStart(2, '0')}`
}

export function VoiceRecorder({ onComplete, disabled }: VoiceRecorderProps) {
  const [isRecording, setIsRecording] = useState(false)
  const [isUploading, setIsUploading] = useState(false)
  const [duration, setDuration] = useState(0)
  const [error, setError] = useState<string | null>(null)
  const mediaRecorderRef = useRef<MediaRecorder | null>(null)
  const chunksRef = useRef<Blob[]>([])
  const autoStopTimerRef = useRef<number | null>(null)
  const durationTimerRef = useRef<number | null>(null)
  const errorTimerRef = useRef<number | null>(null)

  // Duration timer effect
  useEffect(() => {
    if (isRecording) {
      setDuration(0)
      durationTimerRef.current = window.setInterval(() => {
        setDuration((prev) => prev + 1)
      }, 1000)
    } else {
      if (durationTimerRef.current) {
        clearInterval(durationTimerRef.current)
        durationTimerRef.current = null
      }
    }

    return () => {
      if (durationTimerRef.current) {
        clearInterval(durationTimerRef.current)
        durationTimerRef.current = null
      }
    }
  }, [isRecording])

  // Auto-dismiss error after 5 seconds
  useEffect(() => {
    if (error) {
      if (errorTimerRef.current) {
        clearTimeout(errorTimerRef.current)
      }
      errorTimerRef.current = window.setTimeout(() => {
        setError(null)
        errorTimerRef.current = null
      }, 5000)
    }

    return () => {
      if (errorTimerRef.current) {
        clearTimeout(errorTimerRef.current)
        errorTimerRef.current = null
      }
    }
  }, [error])

  async function startRecording() {
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true })
      const mediaRecorder = new MediaRecorder(stream, {
        mimeType: 'audio/webm',
      })

      chunksRef.current = []

      mediaRecorder.ondataavailable = (e) => {
        if (e.data.size > 0) {
          chunksRef.current.push(e.data)
        }
      }

      mediaRecorder.onstop = async () => {
        const blob = new Blob(chunksRef.current, { type: 'audio/webm' })
        await uploadAudio(blob)

        // Stop all tracks to release microphone
        stream.getTracks().forEach((track) => track.stop())
      }

      mediaRecorderRef.current = mediaRecorder
      mediaRecorder.start()
      setIsRecording(true)

      // Auto-stop after 10 minutes to prevent accidental long recordings
      autoStopTimerRef.current = window.setTimeout(() => {
        stopRecording()
      }, 10 * 60 * 1000)
    } catch (err) {
      console.error('Failed to start recording:', err)
      setError('Failed to access microphone. Please allow microphone access.')
    }
  }

  function stopRecording() {
    if (mediaRecorderRef.current && isRecording) {
      mediaRecorderRef.current.stop()
      setIsRecording(false)

      // Clear auto-stop timer
      if (autoStopTimerRef.current) {
        clearTimeout(autoStopTimerRef.current)
        autoStopTimerRef.current = null
      }
    }
  }

  function cancelRecording() {
    if (mediaRecorderRef.current && isRecording) {
      // Stop the recorder without triggering onstop upload
      mediaRecorderRef.current.ondataavailable = null
      mediaRecorderRef.current.onstop = null
      mediaRecorderRef.current.stop()

      // Stop all tracks to release microphone
      mediaRecorderRef.current.stream.getTracks().forEach((track) => track.stop())

      setIsRecording(false)
      chunksRef.current = []

      // Clear auto-stop timer
      if (autoStopTimerRef.current) {
        clearTimeout(autoStopTimerRef.current)
        autoStopTimerRef.current = null
      }
    }
  }

  async function uploadAudio(blob: Blob) {
    setIsUploading(true)

    try {
      const formData = new FormData()
      formData.append('file', blob, 'recording.webm')

      const token = localStorage.getItem('note_token') ?? ''
      const res = await fetch('/api/voice', {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${token}`,
        },
        body: formData,
      })

      if (!res.ok) {
        throw new Error(`Upload failed: ${res.status}`)
      }

      onComplete()
    } catch (err) {
      console.error('Failed to upload audio:', err)
      setError('Failed to upload recording. Please try again.')
    } finally {
      setIsUploading(false)
    }
  }

  return (
    <div className="fixed bottom-6 right-6">
      {/* Error toast */}
      {error && (
        <div className="absolute bottom-20 right-0 w-72 bg-muted-red text-white text-sm px-4 py-3 rounded-lg shadow-lg animate-[fadeInUp_0.2s_ease-out]">
          <div className="flex items-start justify-between gap-2">
            <span>{error}</span>
            <button
              onClick={() => setError(null)}
              className="shrink-0 text-white/80 hover:text-white"
              aria-label="Dismiss error"
            >
              <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
        </div>
      )}

      {!isRecording && !isUploading && (
        <button
          onClick={startRecording}
          disabled={disabled}
          className="w-16 h-16 rounded-full bg-sage-600 text-white shadow-lg hover:bg-sage-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center"
          title="Record voice memo"
          aria-label="Record voice memo"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            className="h-8 w-8"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M19 11a7 7 0 01-7 7m0 0a7 7 0 01-7-7m7 7v4m0 0H8m4 0h4m-4-8a3 3 0 01-3-3V5a3 3 0 116 0v6a3 3 0 01-3 3z"
            />
          </svg>
        </button>
      )}

      {isRecording && (
        <div className="flex items-center gap-3">
          {/* Recording indicator with duration */}
          <div className="flex items-center gap-2 bg-white/95 backdrop-blur-sm rounded-full px-4 py-2 shadow-lg border border-sage-200">
            {/* Pulsing red dot */}
            <span className="relative flex h-3 w-3">
              <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-muted-red opacity-75" />
              <span className="relative inline-flex rounded-full h-3 w-3 bg-muted-red" />
            </span>
            {/* Duration display */}
            <span className="text-sage-800 font-mono text-sm font-medium tabular-nums">
              {formatDuration(duration)}
            </span>
          </div>

          {/* Save button */}
          <button
            onClick={stopRecording}
            className="w-12 h-12 rounded-full bg-sage-600 text-white shadow-lg hover:bg-sage-700 transition-colors flex items-center justify-center"
            title="Save recording"
            aria-label="Save recording"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              className="h-6 w-6"
              fill="currentColor"
              viewBox="0 0 24 24"
            >
              <rect x="6" y="6" width="12" height="12" rx="2" />
            </svg>
          </button>

          {/* Cancel button */}
          <button
            onClick={cancelRecording}
            className="w-12 h-12 rounded-full bg-sage-200 text-sage-700 shadow-lg hover:bg-muted-red hover:text-white transition-colors flex items-center justify-center"
            title="Cancel recording"
            aria-label="Cancel recording"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              className="h-6 w-6"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
            >
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      )}

      {isUploading && (
        <div className="w-16 h-16 rounded-full bg-sage-600 text-white shadow-lg flex items-center justify-center">
          <svg
            className="animate-spin h-8 w-8"
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
          >
            <circle
              className="opacity-25"
              cx="12"
              cy="12"
              r="10"
              stroke="currentColor"
              strokeWidth="4"
            />
            <path
              className="opacity-75"
              fill="currentColor"
              d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
            />
          </svg>
        </div>
      )}
    </div>
  )
}
