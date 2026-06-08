import { render, screen, fireEvent, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { EnhancementsSection } from '../EnhancementsSection'
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'sonner'
import { SettingsProvider } from '@/contexts/SettingsContext'
import { hasApiKey } from '@/utils/keyring'
import { defaultWritingSettings } from '@/types/writing'

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

const enabledAISettings = {
  enabled: true,
  provider: 'openai',
  model: 'gpt-5-mini',
  hasApiKey: true,
}

let rejectWritingSettingsUpdate = false
let aiSettingsResponse = baseAISettings

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
    aiSettingsResponse = baseAISettings
    ;(hasApiKey as ReturnType<typeof vi.fn>).mockResolvedValue(false)
    ;(invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'get_settings') {
        return Promise.resolve(baseAppSettings)
      }
      if (cmd === 'save_settings') {
        return Promise.resolve(undefined)
      }
      if (cmd === 'get_enhancement_options') {
        return Promise.resolve({ preset: 'PersonalDictation' })
      }
      if (cmd === 'update_enhancement_options') {
        return Promise.resolve(undefined)
      }
      if (cmd === 'get_writing_settings') {
        return Promise.resolve(defaultWritingSettings)
      }
      if (cmd === 'update_writing_settings') {
        return rejectWritingSettingsUpdate
          ? Promise.reject(new Error('disk full'))
          : Promise.resolve(undefined)
      }
      if (cmd === 'get_ai_settings') {
        return Promise.resolve(aiSettingsResponse)
      }
      if (cmd === 'get_ai_settings_for_provider') {
        const provider = (args as { provider?: string })?.provider || ''
        return Promise.resolve({ ...aiSettingsResponse, provider })
      }
      if (cmd === 'get_openai_config') {
        return Promise.resolve({ baseUrl: 'https://api.openai.com/v1' })
      }
      if (cmd === 'update_ai_settings') {
        aiSettingsResponse = {
          ...aiSettingsResponse,
          ...(args as typeof aiSettingsResponse),
        }
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
      expect(screen.getByText('Corrections')).toBeInTheDocument()
      expect(screen.getByText('Words & Names')).toBeInTheDocument()
      expect(screen.getByText('Text Shortcuts')).toBeInTheDocument()
      expect(screen.getByText('Voice Commands')).toBeInTheDocument()
      expect(screen.getByRole('button', { name: 'Dictation (no AI)' })).toBeInTheDocument()
      expect(screen.getByRole('button', { name: /Code \(requires AI formatting\)/i })).toBeInTheDocument()
    })
  })

  it('disables AI modes when AI formatting is off', async () => {
    renderWithProviders()

    await waitFor(() => {
      const writingButton = screen.getByRole('button', {
        name: /Writing \(requires AI formatting\)/i,
      })
      expect(writingButton).toBeDisabled()
      expect(writingButton).toHaveAttribute(
        'title',
        'Writing requires AI formatting. Turn on AI formatting above.',
      )
      expect(screen.getByRole('button', { name: 'Dictation (no AI)' })).toBeEnabled()
    })

    expect(
      screen.queryByText(/requires AI formatting\. Turn on AI formatting above or/i),
    ).not.toBeInTheDocument()
  })

  it('hides specific language selection when Personal Dictation is loaded', async () => {
    ;(invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === 'get_settings') {
        return Promise.resolve({
          ...baseAppSettings,
          final_text_language: 'fr',
          transcription_task: 'transcribe',
        })
      }
      if (cmd === 'get_enhancement_options') {
        return Promise.resolve({ preset: 'PersonalDictation' })
      }
      if (cmd === 'get_writing_settings') {
        return Promise.resolve(defaultWritingSettings)
      }
      if (cmd === 'get_ai_settings') {
        return Promise.resolve(baseAISettings)
      }
      if (cmd === 'get_openai_config') {
        return Promise.resolve({ baseUrl: 'https://api.openai.com/v1' })
      }
      return Promise.resolve(undefined)
    })

    renderWithProviders()

    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Same as transcript' })).toBeInTheDocument()
      expect(screen.getByRole('button', { name: 'Specific language' })).toBeDisabled()
      expect(screen.queryByRole('combobox')).not.toBeInTheDocument()
    })
  })

  it('saves mode changes when AI formatting is enabled', async () => {
    aiSettingsResponse = enabledAISettings
    ;(hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(async (providerId: string) =>
      providerId === 'openai',
    )

    const user = userEvent.setup()
    renderWithProviders()

    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Writing' })).toBeEnabled()
    })

    await user.click(screen.getByRole('button', { name: 'Writing' }))

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_enhancement_options', {
        options: { preset: 'Writing' },
      })
    })
  })

  it('switches to Personal Dictation when AI formatting is turned off', async () => {
    aiSettingsResponse = { ...enabledAISettings, enabled: true }
    ;(hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(async (providerId: string) =>
      providerId === 'openai',
    )
    ;(invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'get_enhancement_options') {
        return Promise.resolve({ preset: 'Writing' })
      }
      if (cmd === 'get_ai_settings') {
        return Promise.resolve(aiSettingsResponse)
      }
      if (cmd === 'update_ai_settings') {
        aiSettingsResponse = {
          ...aiSettingsResponse,
          ...(args as typeof aiSettingsResponse),
        }
        return Promise.resolve(undefined)
      }
      if (cmd === 'update_enhancement_options') {
        return Promise.resolve(undefined)
      }
      if (cmd === 'get_settings') {
        return Promise.resolve({
          ...baseAppSettings,
          final_text_language: 'fr',
          transcription_task: 'transcribe',
        })
      }
      if (cmd === 'get_writing_settings') {
        return Promise.resolve({
          replacements: [],
          custom_words: [],
          snippets: [],
          context_policy: 'off',
        })
      }
      if (cmd === 'get_ai_settings_for_provider') {
        const provider = (args as { provider?: string })?.provider || ''
        return Promise.resolve({ ...aiSettingsResponse, provider, hasApiKey: provider === 'openai' })
      }
      if (cmd === 'get_openai_config') {
        return Promise.resolve({ baseUrl: 'https://api.openai.com/v1' })
      }
      if (cmd === 'cache_ai_api_key') {
        return Promise.resolve(undefined)
      }
      return Promise.resolve(undefined)
    })

    const user = userEvent.setup()
    renderWithProviders()

    const aiToggle = await screen.findByRole('switch', { name: /ai formatting/i })
    await waitFor(() => expect(aiToggle).toBeEnabled())
    await user.click(aiToggle)

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_enhancement_options', {
        options: { preset: 'PersonalDictation' },
      })
      expect(invoke).toHaveBeenCalledWith('save_settings', {
        settings: expect.objectContaining({
          final_text_language: 'same_as_transcript',
        }),
      })
      expect(toast.success).toHaveBeenCalledWith(
        'AI formatting disabled. Switched to Dictation (no AI).',
      )
    })
  })

  it('switches to Clean Dictation when AI formatting is turned on from Personal Dictation', async () => {
    aiSettingsResponse = { ...enabledAISettings, enabled: false }
    ;(hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(async (providerId: string) =>
      providerId === 'openai',
    )
    const user = userEvent.setup()
    renderWithProviders()

    const aiToggle = await screen.findByRole('switch', { name: /ai formatting/i })
    await waitFor(() => expect(aiToggle).toBeEnabled())
    await user.click(aiToggle)

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_enhancement_options', {
        options: { preset: 'CleanDictation' },
      })
    })
  })

  it('saves custom provider setup without enabling AI formatting', async () => {
    aiSettingsResponse = { ...baseAISettings, provider: '', model: '' }
    const user = userEvent.setup()
    renderWithProviders()

    await user.click(await screen.findByRole('button', { name: 'Configure' }))
    await user.type(await screen.findByLabelText('Model ID'), 'local-model')
    await user.click(screen.getByRole('button', { name: 'Test' }))

    await waitFor(() => {
      expect(screen.getByText('Connection successful')).toBeInTheDocument()
    })

    await user.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_ai_settings', {
        enabled: false,
        provider: 'custom',
        model: 'local-model',
      })
    })
    expect(invoke).not.toHaveBeenCalledWith('update_enhancement_options', {
      options: { preset: 'CleanDictation' },
    })
  })

  it('saves final text language changes through save_settings', async () => {
    aiSettingsResponse = enabledAISettings
    ;(hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(async (providerId: string) =>
      providerId === 'openai',
    )
    const user = userEvent.setup()
    renderWithProviders()

    await user.click(await screen.findByRole('button', { name: 'Clean Dictation' }))
    await user.click(await screen.findByRole('button', { name: 'Specific language' }))

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

  it('does not persist placeholder writing settings before backend settings load', async () => {
    const user = userEvent.setup()
    let resolveWritingSettings: (settings: typeof defaultWritingSettings) => void = () => {}
    const loadedWritingSettings = {
      ...defaultWritingSettings,
      voice_commands: [
        {
          phrase: 'insert period',
          output: 'period',
          language: 'en',
          enabled: true,
        },
      ],
    }
    const writingSettingsPromise = new Promise<typeof defaultWritingSettings>((resolve) => {
      resolveWritingSettings = resolve
    })

    ;(invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'get_settings') {
        return Promise.resolve(baseAppSettings)
      }
      if (cmd === 'save_settings') {
        return Promise.resolve(undefined)
      }
      if (cmd === 'get_enhancement_options') {
        return Promise.resolve({ preset: 'PersonalDictation' })
      }
      if (cmd === 'update_enhancement_options') {
        return Promise.resolve(undefined)
      }
      if (cmd === 'get_writing_settings') {
        return writingSettingsPromise
      }
      if (cmd === 'update_writing_settings') {
        return Promise.resolve(undefined)
      }
      if (cmd === 'get_ai_settings') {
        return Promise.resolve(aiSettingsResponse)
      }
      if (cmd === 'get_ai_settings_for_provider') {
        const provider = (args as { provider?: string })?.provider || ''
        return Promise.resolve({ ...aiSettingsResponse, provider })
      }
      if (cmd === 'get_openai_config') {
        return Promise.resolve({ baseUrl: 'https://api.openai.com/v1' })
      }
      if (cmd === 'update_ai_settings') {
        aiSettingsResponse = {
          ...aiSettingsResponse,
          ...(args as typeof aiSettingsResponse),
        }
        return Promise.resolve(undefined)
      }
      if (cmd === 'cache_ai_api_key') {
        return Promise.resolve(undefined)
      }
      return Promise.resolve(undefined)
    })

    renderWithProviders()

    const switches = await screen.findAllByRole('switch')
    const contextSwitch = switches[1]
    expect(contextSwitch).toBeDisabled()
    fireEvent.click(contextSwitch)
    expect(
      (invoke as ReturnType<typeof vi.fn>).mock.calls.some(
        ([cmd, args]) =>
          cmd === 'update_writing_settings' &&
          (args as { settings?: typeof defaultWritingSettings })?.settings?.voice_commands.length === 0,
      ),
    ).toBe(false)

    resolveWritingSettings(loadedWritingSettings)
    await waitFor(() => expect(contextSwitch).toBeEnabled())
    await user.click(contextSwitch)

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_writing_settings', {
        settings: expect.objectContaining({
          context_policy: 'app_hint_only',
          voice_commands: loadedWritingSettings.voice_commands,
        }),
      })
    })
  })

  it('adds an app formatting rule and persists writing settings', async () => {
    const user = userEvent.setup()
    renderWithProviders()

    const appRulesHeading = await screen.findByText('App Rules')
    const appRulesCard = appRulesHeading.parentElement?.parentElement
    expect(appRulesCard).toBeTruthy()

    await user.click(
      within(appRulesCard as HTMLElement).getByRole('button', { name: /add rule/i }),
    )

    const appInput = await screen.findByPlaceholderText('App name, e.g. Slack')
    await user.type(appInput, 'Slack')

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_writing_settings', {
        settings: expect.objectContaining({
          app_formatting_rules: [
            expect.objectContaining({
              app_name: 'Slack',
              preset: 'PersonalDictation',
              enabled: true,
            }),
          ],
        }),
      })
    })
  })

  it('adds a text replacement row and persists writing settings', async () => {
    const user = userEvent.setup()
    renderWithProviders()

    const replacementsHeading = await screen.findByText('Corrections')
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

  it('adds a voice command row and persists writing settings', async () => {
    const user = userEvent.setup()
    renderWithProviders()

    const voiceCommandsHeading = await screen.findByText('Voice Commands')
    const voiceCommandsCard = voiceCommandsHeading.parentElement?.parentElement
    expect(voiceCommandsCard).toBeTruthy()

    await user.click(
      within(voiceCommandsCard as HTMLElement).getByRole('button', { name: /add command/i }),
    )

    fireEvent.change(await screen.findByLabelText('Voice command phrase 1'), {
      target: { value: 'new paragraph' },
    })
    fireEvent.change(screen.getByLabelText('Voice command language 1'), {
      target: { value: 'en' },
    })
    await user.click(screen.getByRole('switch', { name: 'Enable voice command 1' }))

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_writing_settings', {
        settings: expect.objectContaining({
          voice_commands: [
            expect.objectContaining({
              phrase: 'new paragraph',
              output: 'period',
              language: 'en',
              enabled: false,
            }),
          ],
        }),
      })
    })
  })

  it('coalesces rapid writing settings saves so the latest edit wins on disk', async () => {
    const user = userEvent.setup()
    let resolveFirstSave: (() => void) | undefined
    const firstSaveGate = new Promise<void>((resolve) => {
      resolveFirstSave = resolve
    })
    let firstSaveStarted = false

    ;(invoke as ReturnType<typeof vi.fn>).mockImplementation(
      (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === 'get_settings') {
          return Promise.resolve(baseAppSettings)
        }
        if (cmd === 'save_settings') {
          return Promise.resolve(undefined)
        }
        if (cmd === 'get_enhancement_options') {
          return Promise.resolve({ preset: 'PersonalDictation' })
        }
        if (cmd === 'update_enhancement_options') {
          return Promise.resolve(undefined)
        }
        if (cmd === 'get_writing_settings') {
          return Promise.resolve(defaultWritingSettings)
        }
        if (cmd === 'update_writing_settings') {
          if (!firstSaveStarted) {
            firstSaveStarted = true
            return firstSaveGate.then(() => Promise.resolve(undefined))
          }
          return Promise.resolve(undefined)
        }
        if (cmd === 'get_ai_settings') {
          return Promise.resolve(aiSettingsResponse)
        }
        if (cmd === 'get_ai_settings_for_provider') {
          const provider = (args as { provider?: string })?.provider || ''
          return Promise.resolve({ ...aiSettingsResponse, provider })
        }
        if (cmd === 'get_openai_config') {
          return Promise.resolve({ baseUrl: 'https://api.openai.com/v1' })
        }
        if (cmd === 'update_ai_settings') {
          aiSettingsResponse = {
            ...aiSettingsResponse,
            ...(args as typeof aiSettingsResponse),
          }
          return Promise.resolve(undefined)
        }
        if (cmd === 'cache_ai_api_key') {
          return Promise.resolve(undefined)
        }
        return Promise.resolve(undefined)
      },
    )

    renderWithProviders()

    const replacementsHeading = await screen.findByText('Corrections')
    const replacementsCard = replacementsHeading.parentElement?.parentElement
    expect(replacementsCard).toBeTruthy()
    const addRuleButton = within(replacementsCard as HTMLElement).getByRole('button', {
      name: /add/i,
    })

    await user.click(addRuleButton)
    await user.click(addRuleButton)

    resolveFirstSave?.()

    await waitFor(() => {
      const updateCalls = (invoke as ReturnType<typeof vi.fn>).mock.calls.filter(
        ([cmd]) => cmd === 'update_writing_settings',
      )
      expect(updateCalls).toHaveLength(2)
      expect(updateCalls[1]?.[1]).toEqual({
        settings: expect.objectContaining({
          replacements: expect.arrayContaining([
            expect.objectContaining({ enabled: true }),
            expect.objectContaining({ enabled: true }),
          ]),
        }),
      })
    })
  })

  it('rolls back optimistic writing settings when save fails', async () => {
    const user = userEvent.setup()
    rejectWritingSettingsUpdate = true
    renderWithProviders()

    const replacementsHeading = await screen.findByText('Corrections')
    const replacementsCard = replacementsHeading.parentElement?.parentElement
    expect(replacementsCard).toBeTruthy()

    await user.click(
      within(replacementsCard as HTMLElement).getByRole('button', { name: /add/i }),
    )

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith('disk full')
    })
    await waitFor(() => {
      expect(screen.queryByText('Rule 1')).not.toBeInTheDocument()
    })
  })

  it('shows formatting setup guidance in the guide dialog', async () => {
    const user = userEvent.setup()
    renderWithProviders()

    await user.click(await screen.findByRole('button', { name: /formatting guide/i }))

    await waitFor(() => {
      const dialog = screen.getByRole('dialog')
      expect(within(dialog).getByText(/set up one provider, save its API key/i)).toBeInTheDocument()
      expect(within(dialog).getByText(/Personal Dictation/i)).toBeInTheDocument()
      expect(toast.error).not.toHaveBeenCalled()
    })
  })
})
