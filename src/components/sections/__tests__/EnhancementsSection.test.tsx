import { render, screen, fireEvent, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { EnhancementsSection } from '../EnhancementsSection'
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'sonner'
import { SettingsProvider } from '@/contexts/SettingsContext'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))

vi.mock('sonner', () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn(),
    info: vi.fn(),
  },
}))

vi.mock('@/utils/keyring', () => ({
  saveApiKey: vi.fn().mockResolvedValue(undefined),
  hasApiKey: vi.fn().mockResolvedValue(false),
  removeApiKey: vi.fn().mockResolvedValue(undefined),
  getApiKey: vi.fn().mockResolvedValue(null),
}))

vi.mock('@/hooks/useProviderModels', () => ({
  useAllProviderModels: () => ({
    fetchModels: vi.fn(),
    getModels: (providerId: string) => {
      if (providerId === 'gemini') {
        return [{ id: 'gemini-1.5-flash', name: 'Gemini 1.5 Flash' }]
      }
      if (providerId === 'openai') {
        return [{ id: 'gpt-5-mini', name: 'GPT-5 Mini' }]
      }
      return []
    },
    isLoading: () => false,
    getError: () => null,
    clearModels: vi.fn(),
  }),
}))

const baseAISettings = {
  enabled: false,
  provider: '',
  model: '',
  hasApiKey: false,
}


let rejectWritingSettingsUpdate = false

const baseAppSettings = {
  hotkey: 'CommandOrControl+Shift+Space',
  current_model: 'base',
  current_model_engine: 'whisper',
  speech_language: 'en',
  transcription_task: 'transcribe',
  final_text_language: 'same_as_transcript',
  theme: 'system',
}

function renderWithProviders() {
  return render(
    <SettingsProvider>
      <EnhancementsSection />
    </SettingsProvider>,
  )
}

describe('EnhancementsSection', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    rejectWritingSettingsUpdate = false
    ;(invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'get_settings') {
        return Promise.resolve(baseAppSettings)
      }
      if (cmd === 'save_settings') {
        return Promise.resolve(undefined)
      }
      if (cmd === 'get_enhancement_options') {
        return Promise.resolve({ preset: 'Default' })
      }
      if (cmd === 'update_enhancement_options') {
        return Promise.resolve(undefined)
      }
      if (cmd === 'get_writing_settings') {
        return Promise.resolve({
          replacements: [],
          custom_words: [],
          snippets: [],
          context_policy: 'off',
        })
      }
      if (cmd === 'update_writing_settings') {
        return rejectWritingSettingsUpdate
          ? Promise.reject(new Error('disk full'))
          : Promise.resolve(undefined)
      }
      if (cmd === 'get_ai_settings') {
        return Promise.resolve(baseAISettings)
      }
      if (cmd === 'get_ai_settings_for_provider') {
        const provider = (args as { provider?: string })?.provider || ''
        return Promise.resolve({ ...baseAISettings, provider })
      }
      if (cmd === 'get_openai_config') {
        return Promise.resolve({ baseUrl: 'https://api.openai.com/v1' })
      }
      if (cmd === 'update_ai_settings') {
        return Promise.resolve(undefined)
      }
      if (cmd === 'cache_ai_api_key') {
        return Promise.resolve(undefined)
      }
      return Promise.resolve(undefined)
    })
  })

  it('renders providers and writing controls', async () => {
    renderWithProviders()

    await waitFor(() => {
      expect(screen.getByText('AI Providers')).toBeInTheDocument()
      expect(screen.getByText('Formatting Options')).toBeInTheDocument()
      expect(screen.getByText('Text replacements')).toBeInTheDocument()
      expect(screen.getByText('Personal dictionary words')).toBeInTheDocument()
      expect(screen.getByText('Snippets')).toBeInTheDocument()
    })
  })

  it('saves preset changes', async () => {
    const user = userEvent.setup()
    renderWithProviders()

    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Email' })).toBeInTheDocument()
    })

    await user.click(screen.getByRole('button', { name: 'Email' }))

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_enhancement_options', {
        options: { preset: 'Email' },
      })
    })
  })

  it('saves final text language changes through save_settings', async () => {
    const user = userEvent.setup()
    renderWithProviders()

    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Specific language' })).toBeInTheDocument()
    })

    await user.click(screen.getByRole('button', { name: 'Specific language' }))

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('save_settings', {
        settings: expect.objectContaining({
          final_text_language: 'en',
          transcription_task: 'translate_to_english',
        }),
      })
    })
  })

  it('saves context-aware cleanup changes', async () => {
    renderWithProviders()

    const switches = await screen.findAllByRole('switch')
    fireEvent.click(switches[1])

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_writing_settings', {
        settings: expect.objectContaining({
          context_policy: 'app_hint_only',
        }),
      })
    })
  })

  it('adds a text replacement row and persists writing settings', async () => {
    const user = userEvent.setup()
    renderWithProviders()

    const replacementsHeading = await screen.findByText('Text replacements')
    const replacementsCard = replacementsHeading.parentElement?.parentElement
    expect(replacementsCard).toBeTruthy()

    await user.click(
      within(replacementsCard as HTMLElement).getByRole('button', { name: /add/i }),
    )

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_writing_settings', {
        settings: expect.objectContaining({
          replacements: [
            expect.objectContaining({
              from: '',
              to: '',
              enabled: true,
            }),
          ],
        }),
      })
    })
  })

  it('rolls back optimistic writing settings when save fails', async () => {
    const user = userEvent.setup()
    rejectWritingSettingsUpdate = true
    renderWithProviders()

    const replacementsHeading = await screen.findByText('Text replacements')
    const replacementsCard = replacementsHeading.parentElement?.parentElement
    expect(replacementsCard).toBeTruthy()

    await user.click(
      within(replacementsCard as HTMLElement).getByRole('button', { name: /add/i }),
    )

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith('disk full')
    })
    await waitFor(() => {
      expect(screen.queryByText('Replacement 1')).not.toBeInTheDocument()
    })
  })


  it('shows guidance when AI is disabled', async () => {
    renderWithProviders()

    await waitFor(() => {
      expect(screen.getByText('Quick Setup')).toBeInTheDocument()
      expect(toast.error).not.toHaveBeenCalled()
    })
  })
})
