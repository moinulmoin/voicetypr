import { create } from 'zustand'
import { invoke } from '@tauri-apps/api/core'

export type UploadStatus = 'idle' | 'processing' | 'done' | 'error'

export type SelectedFile = { path: string; name: string }

type UploadState = {
  selectedFile: SelectedFile | null
  status: UploadStatus
  resultText: string | null
  error: string | null
  select: (path: string) => void
  clearSelection: () => void
  start: (modelName: string, modelEngine: string) => Promise<void>
  reset: () => void
}

export const useUploadStore = create<UploadState>((set, get) => ({
  selectedFile: null,
  status: 'idle',
  resultText: null,
  error: null,

  select: (path: string) => {
    const name = path.split('/').pop() || path.split('\\').pop() || 'audio file'
    set({ selectedFile: { path, name }, resultText: null, error: null })
  },

  clearSelection: () => set({ selectedFile: null }),

  start: async (modelName: string, modelEngine: string) => {
    const { selectedFile, status } = get()
    if (!selectedFile) return
    if (status === 'processing') return
    set({ status: 'processing', error: null, resultText: null })
    try {
      const text = await invoke<string>('transcribe_audio_file', {
        filePath: selectedFile.path,
        modelName,
        modelEngine,
      })
      if (!text || text.trim() === '' || text === '[BLANK_AUDIO]') {
        set({ status: 'error', error: 'No speech detected in the audio file' })
        return
      }

      await invoke('save_transcription', { text, model: modelName })
      set({ status: 'done', resultText: text })
    } catch (e: any) {
      set({ status: 'error', error: String(e?.message || e) })
    }
  },

  reset: () => set({ selectedFile: null, status: 'idle', resultText: null, error: null })
}))
