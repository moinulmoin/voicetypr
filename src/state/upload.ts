import { create } from 'zustand'
import { invoke } from '@tauri-apps/api/core'

export type UploadStatus = 'idle' | 'processing' | 'done' | 'error'
export type UploadResult =
  | { outcome: 'success'; text: string }
  | { outcome: 'blank' }
  | { outcome: 'error'; message: string }

export type SelectedFile = { path: string; name: string }
export type SpeakerSegment = { speaker_id: string; start_ms: number; end_ms: number }

type UploadState = {
  selectedFile: SelectedFile | null
  status: UploadStatus
  resultText: string | null
  error: string | null
  speakerSegments: SpeakerSegment[]
  diarizationError: string | null
  select: (path: string) => void
  clearSelection: () => void
  start: (modelName: string, modelEngine: string | null, historyModelName?: string) => Promise<UploadResult | null>
  reset: () => void
}

export const useUploadStore = create<UploadState>((set, get) => ({
  selectedFile: null,
  status: 'idle',
  resultText: null,
  error: null,
  speakerSegments: [],
  diarizationError: null,

  select: (path: string) => {
    const name = path.split(/[\\/]/).pop() || 'audio file'
    set({ selectedFile: { path, name }, resultText: null, error: null, speakerSegments: [], diarizationError: null })
  },

  clearSelection: () => set({ selectedFile: null, speakerSegments: [], diarizationError: null }),

  start: async (modelName: string, modelEngine: string | null, historyModelName?: string) => {
    const { selectedFile, status } = get()
    if (!selectedFile) return null
    if (status === 'processing') return null
    set({ status: 'processing', error: null, resultText: null, speakerSegments: [], diarizationError: null })
    try {
      const text = await invoke<string>('transcribe_audio_file', {
        filePath: selectedFile.path,
        modelName,
        modelEngine,
      })
      if (!text || text.trim() === '' || text === '[BLANK_AUDIO]') {
        set({ status: 'error', error: 'No speech detected in the audio file' })
        return { outcome: 'blank' }
      }

      let speakerSegments: SpeakerSegment[] = []
      let diarizationError: string | null = null
      if (modelEngine === 'parakeet') {
        try {
          speakerSegments = await invoke<SpeakerSegment[]>('diarize_audio_file', {
            filePath: selectedFile.path,
          })
        } catch (error) {
          diarizationError = String(error)
        }
      }

      await invoke('save_transcription', {
        text,
        model: historyModelName || modelName,
      })
      set({ status: 'done', resultText: text, speakerSegments, diarizationError })
      return { outcome: 'success', text }
    } catch (e: any) {
      const message = String(e?.message || e)
      set({ status: 'error', error: message })
      return { outcome: 'error', message }
    }
  },

  reset: () => set({ selectedFile: null, status: 'idle', resultText: null, error: null, speakerSegments: [], diarizationError: null })
}))
