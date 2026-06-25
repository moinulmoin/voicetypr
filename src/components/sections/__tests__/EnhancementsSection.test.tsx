import { render, screen, fireEvent, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { EnhancementsSection } from '../EnhancementsSection'
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'sonner'
import { SettingsProvider } from '@/contexts/SettingsContext'
import { hasApiKey, saveApiKey } from '@/utils/keyring'
import { defaultWritingSettings, mergeWritingSettings } from '@/types/writing'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

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

const providerModels = vi.hoisted(
  (): Record<string, Array<{
    id: string
    name: string
    recommended: boolean
    reasoning?: boolean
    contextWindow?: number | null
    costInput?: number | null
    costOutput?: number | null
  }>> => ({
    gemini: [{ id: 'gemini-1.5-flash', name: 'Gemini 1.5 Flash', recommended: true }],
    openai: [
      {
        id: 'gpt-5-mini',
        name: 'GPT-5 Mini',
        recommended: true,
        reasoning: true,
        contextWindow: 400000,
        costInput: 0.25,
        costOutput: 2,
      },
      { id: 'gpt-5-nano', name: 'GPT-5 Nano', recommended: false },
    ],
    anthropic: [{ id: 'claude-sonnet-4', name: 'Claude Sonnet 4', recommended: true }],
    groq: [{ id: 'llama-3.3-70b-versatile', name: 'Llama 3.3 70B Versatile', recommended: true }],
    deepseek: [{ id: 'deepseek-chat', name: 'DeepSeek Chat', recommended: true }],
  }),
)

vi.mock('@/hooks/useProviderModels', () => ({
  useAllProviderModels: () => ({
    fetchModels: vi.fn(),
    getModels: (providerId: string) => providerModels[providerId] || [],
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
  modelsByProvider: {},
  aiModelNeedsReselection: false,
}

const enabledAISettings = {
  enabled: true,
  provider: 'openai',
  model: 'gpt-5-mini',
  hasApiKey: true,
  modelsByProvider: {
    openai: 'gpt-5-mini',
  },
  aiModelNeedsReselection: false,
}

const providerListResponse = [
  { id: 'openai', name: 'OpenAI', status: 'production', supportsReasoning: true },
  { id: 'gemini', name: 'Google Gemini', status: 'production', supportsReasoning: true },
  { id: 'anthropic', name: 'Anthropic', status: 'production', supportsReasoning: true },
  { id: 'custom', name: 'Custom (OpenAI-compatible)', status: 'production', supportsBaseUrl: true },
  { id: 'groq', name: 'Groq', status: 'experimental', supportsReasoning: false },
  { id: 'deepseek', name: 'DeepSeek', status: 'hidden', supportsReasoning: false },
]

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
      if (cmd === 'list_ai_providers') {
        return Promise.resolve(providerListResponse)
      }
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
        const nextAISettings = args as typeof aiSettingsResponse
        aiSettingsResponse = {
          ...aiSettingsResponse,
          ...nextAISettings,
          aiModelNeedsReselection: nextAISettings.model
            ? false
            : aiSettingsResponse.aiModelNeedsReselection,
        }

        return Promise.resolve(undefined)
      }
      if (cmd === 'cache_ai_api_key') {
        return Promise.resolve(undefined)
      }
      return Promise.resolve(undefined)
    })
  })

  it('renders production providers, experimental badges, and gates hidden providers behind Advanced', async () => {
    const user = userEvent.setup()
    renderWithProviders()

    expect(await screen.findByText('OpenAI')).toBeInTheDocument()
    expect(screen.getByText('Google Gemini')).toBeInTheDocument()
    expect(screen.getByText('Anthropic')).toBeInTheDocument()
    expect(screen.getByText('Custom (OpenAI-compatible)')).toBeInTheDocument()
    expect(screen.getByText('Groq')).toBeInTheDocument()
    expect(screen.getByText('Experimental')).toBeInTheDocument()
    expect(screen.queryByText('DeepSeek')).not.toBeInTheDocument()

    await user.click(screen.getByRole('switch', { name: /show advanced ai providers/i }))

    expect(await screen.findByText('DeepSeek')).toBeInTheDocument()
  })

  it('filters providers and grouped models by search text', async () => {
    ;(hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(async (providerId: string) =>
      providerId === 'openai' || providerId === 'groq',
    )
    const user = userEvent.setup()
    renderWithProviders()

    await user.type(await screen.findByLabelText('Search providers and models'), 'llama')

    await waitFor(() => {
      expect(screen.queryByText('OpenAI')).not.toBeInTheDocument()
      expect(screen.getByText('Groq')).toBeInTheDocument()
      expect(screen.getByRole('button', { name: /llama 3\.3 70b versatile/i })).toBeInTheDocument()
      expect(screen.getByText('Recommended')).toBeInTheDocument()
    })
  })

  it('persists selected model from the grouped picker', async () => {
    ;(hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(async (providerId: string) =>
      providerId === 'openai',
    )
    const user = userEvent.setup()
    renderWithProviders()

    await user.click(await screen.findByRole('button', { name: /gpt-5 nano/i }))

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_ai_settings', {
        enabled: false,
        provider: 'openai',
        model: 'gpt-5-nano',
      })
    })
  })

  it('renders providers and writing controls', async () => {
    renderWithProviders()

    await waitFor(() => {
      expect(screen.getByText('AI polish (optional)')).toBeInTheDocument()
      expect(screen.getByText('Your text rules (always on)')).toBeInTheDocument()
      expect(screen.getByText('AI Providers')).toBeInTheDocument()
      expect(screen.getByText('Corrections')).toBeInTheDocument()
      expect(screen.getByText('Words & Names')).toBeInTheDocument()
      expect(screen.getByText('Text Shortcuts')).toBeInTheDocument()
      expect(screen.getByRole('button', { name: 'Personal Dictation' })).toBeInTheDocument()
      expect(screen.getByRole('button', { name: /Code \(requires AI formatting\)/i })).toBeInTheDocument()
      expect(screen.getByText('OpenAI')).toBeInTheDocument()
      expect(screen.getByText('Google Gemini')).toBeInTheDocument()
      expect(invoke).toHaveBeenCalledWith('list_ai_providers')
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
        'Writing requires AI formatting. Turn on AI formatting with a selected provider model.',
      )
      expect(screen.getByRole('button', { name: 'Personal Dictation' })).toBeEnabled()
    })

    expect(
      screen.queryByText(/requires AI formatting\. Turn on AI formatting above or/i),
    ).not.toBeInTheDocument()
  })

  it('explains how to enable AI formatting when setup is incomplete', async () => {
    renderWithProviders()

    await waitFor(() => {
      expect(
        screen.getByText('Rewrites your words for meaning and format. Needs a provider. Off by default.'),
      ).toBeInTheDocument()
      expect(
        screen.getByText('Add an API key and choose a model below to turn on AI formatting.'),
      ).toBeInTheDocument()
      expect(screen.getByRole('switch', { name: /ai formatting/i })).toBeDisabled()
    })
  })

  it('shows the selected model when AI formatting is off', async () => {
    aiSettingsResponse = { ...enabledAISettings, enabled: false }
    ;(hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(async (providerId: string) =>
      providerId === 'openai',
    )

    renderWithProviders()

    await waitFor(() => {
      expect(
        screen.getAllByText((_, element) =>
          element?.textContent === 'Selected model: GPT-5 Mini (AI formatting off)',
        ).length,
      ).toBeGreaterThan(0)
      expect(screen.queryByText('Add an API key and choose a model below to turn on AI formatting.')).not.toBeInTheDocument()
    })
  })

  it('hides specific language selection when Personal Dictation is loaded', async () => {
    ;(invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === 'list_ai_providers') {
        return Promise.resolve(providerListResponse)
      }
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
        return Promise.resolve({ ...defaultWritingSettings, voice_commands: [] })
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

  it('rolls back optimistic mode changes when preset persistence fails', async () => {
    aiSettingsResponse = enabledAISettings
    ;(hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(async (providerId: string) =>
      providerId === 'openai',
    )
    ;(invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'list_ai_providers') {
        return Promise.resolve(providerListResponse)
      }
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
        return Promise.reject(new Error('preset save failed'))
      }
      if (cmd === 'get_writing_settings') {
        return Promise.resolve(defaultWritingSettings)
      }
      if (cmd === 'get_ai_settings') {
        return Promise.resolve(aiSettingsResponse)
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

    await user.click(await screen.findByRole('button', { name: 'Writing' }))

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith('preset save failed')
      expect(
        screen.getByText(/Just transcription with local cleanup/i),
      ).toBeInTheDocument()
    })
  })

  it('switches to Personal Dictation when AI formatting is turned off', async () => {
    aiSettingsResponse = { ...enabledAISettings, enabled: true }
    ;(hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(async (providerId: string) =>
      providerId === 'openai',
    )
    ;(invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'list_ai_providers') {
        return Promise.resolve(providerListResponse)
      }
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
        'AI formatting disabled. Switched to Personal Dictation.',
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

  it('renders the two labeled zones with their copy', async () => {
    renderWithProviders()

    await waitFor(() => {
      expect(screen.getByText('AI polish (optional)')).toBeInTheDocument()
      expect(
        screen.getByText(
          'Rewrites your words for meaning and format. Needs a provider. Off by default.',
        ),
      ).toBeInTheDocument()
      expect(screen.getByText('Your text rules (always on)')).toBeInTheDocument()
      expect(
        screen.getByText(
          'Exact, predictable edits. Run on every transcription, with or without AI.',
        ),
      ).toBeInTheDocument()
    })
  })

  it('does not render a context_policy control after the app-hint removal', async () => {
    renderWithProviders()

    await waitFor(() => expect(screen.getByText('AI Providers')).toBeInTheDocument())
    expect(
      screen.queryByRole('switch', { name: 'Context-aware cleanup' }),
    ).not.toBeInTheDocument()
  })

  it('renders the deterministic editors in Zone B with AI off', async () => {
    renderWithProviders()

    await waitFor(() => expect(screen.getByText('Corrections')).toBeInTheDocument())
    expect(screen.getByText('Words & Names')).toBeInTheDocument()
    expect(screen.getByText('Text Shortcuts')).toBeInTheDocument()
  })

  it('keeps AiProviderStatus single-sourced and drops the app-hint field on merge', () => {
    const projectRoot = path.resolve(
      path.dirname(fileURLToPath(import.meta.url)),
      '..',
      '..',
      '..',
      '..',
    )
    const aiSrc = readFileSync(path.join(projectRoot, 'src/types/ai.ts'), 'utf8')
    const providersSrc = readFileSync(path.join(projectRoot, 'src/types/providers.ts'), 'utf8')
    expect(providersSrc).toMatch(/export type AiProviderStatus/)
    expect(aiSrc).not.toMatch(/AiProviderStatus/)

    const legacy = {
      replacements: [{ from: 'x', to: 'y', language: null, enabled: true }],
      custom_words: [],
      snippets: [],
      voice_commands: [],
      context_policy: 'app_hint_only',
    } as unknown as Partial<typeof defaultWritingSettings>
    const merged = mergeWritingSettings(legacy)
    expect(merged.replacements).toHaveLength(1)
    expect(merged.app_formatting_rules).toEqual([])
    expect('context_policy' in merged).toBe(false)
    expect(mergeWritingSettings({})).toEqual(defaultWritingSettings)
  })

  it('deletes the unused ProviderCard component', () => {
    const projectRoot = path.resolve(
      path.dirname(fileURLToPath(import.meta.url)),
      '..',
      '..',
      '..',
      '..',
    )
    expect(() =>
      readFileSync(path.join(projectRoot, 'src/components/ProviderCard.tsx'), 'utf8'),
    ).toThrow()
  })

  it('does not persist placeholder writing settings before backend settings load', async () => {
    const user = userEvent.setup()
    let resolveWritingSettings: (settings: typeof defaultWritingSettings) => void = () => {}
    const loadedWritingSettings = {
      ...defaultWritingSettings,
      replacements: [
        {
          from: 'voice typer',
          to: 'Voicetypr',
          language: null,
          enabled: true,
        },
      ],
    }
    const writingSettingsPromise = new Promise<typeof defaultWritingSettings>((resolve) => {
      resolveWritingSettings = resolve
    })

    ;(invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'list_ai_providers') {
        return Promise.resolve(providerListResponse)
      }
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

    const correctionsHeading = await screen.findByText('Corrections')
    const correctionsCard = correctionsHeading.parentElement?.parentElement
    expect(correctionsCard).toBeTruthy()
    const addRuleButton = within(correctionsCard as HTMLElement).getByRole('button', {
      name: /add rule/i,
    })
    expect(addRuleButton).toBeDisabled()
    fireEvent.click(addRuleButton)
    expect(
      (invoke as ReturnType<typeof vi.fn>).mock.calls.some(
        ([cmd, args]) =>
          cmd === 'update_writing_settings' &&
          (args as { settings?: typeof defaultWritingSettings })?.settings?.replacements.length === 0,
      ),
    ).toBe(false)

    resolveWritingSettings(loadedWritingSettings)
    await waitFor(() => expect(addRuleButton).toBeEnabled())
    await user.click(addRuleButton)

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_writing_settings', {
        settings: expect.objectContaining({
          replacements: [
            ...loadedWritingSettings.replacements,
            expect.objectContaining({ from: '', to: '', enabled: true }),
          ],
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

  it.skip('adds a voice command row and persists writing settings (Voice Commands removed from UI)', async () => {
    const user = userEvent.setup()
    // Built-in voice commands now ship in `defaultWritingSettings` (mirroring
    // the Rust serde default). This test exercises add-row/persist from an
    // empty list, so load writing settings without the built-ins.
    ;(invoke as ReturnType<typeof vi.fn>).mockImplementation(
      (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === 'list_ai_providers') {
          return Promise.resolve(providerListResponse)
        }
        if (cmd === 'get_settings') {
          return Promise.resolve(baseAppSettings)
        }
        if (cmd === 'get_enhancement_options') {
          return Promise.resolve({ preset: 'PersonalDictation' })
        }
        if (cmd === 'get_writing_settings') {
          return Promise.resolve({ ...defaultWritingSettings, voice_commands: [] })
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
        return Promise.resolve(undefined)
      },
    )
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
        if (cmd === 'list_ai_providers') {
          return Promise.resolve(providerListResponse)
        }
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

  it('saves each queued writing settings snapshot instead of the latest ref for every save', async () => {
    const user = userEvent.setup()
    renderWithProviders()

    const replacementsHeading = await screen.findByText('Corrections')
    const replacementsCard = replacementsHeading.parentElement?.parentElement
    expect(replacementsCard).toBeTruthy()
    const addRuleButton = within(replacementsCard as HTMLElement).getByRole('button', {
      name: /add/i,
    })

    await user.click(addRuleButton)
    await user.click(addRuleButton)

    await waitFor(() => {
      const updateCalls = (invoke as ReturnType<typeof vi.fn>).mock.calls.filter(
        ([cmd]) => cmd === 'update_writing_settings',
      )
      expect(updateCalls).toHaveLength(2)
      expect(
        (updateCalls[0]?.[1] as { settings: typeof defaultWritingSettings }).settings.replacements,
      ).toHaveLength(1)
      expect(
        (updateCalls[1]?.[1] as { settings: typeof defaultWritingSettings }).settings.replacements,
      ).toHaveLength(2)
    })
  })

  it('does not roll back writing settings when an older queued save fails after a newer edit', async () => {
    const user = userEvent.setup()
    let rejectFirstSave: (() => void) | undefined
    const firstSaveGate = new Promise<void>((_, reject) => {
      rejectFirstSave = () => reject(new Error('stale save failed'))
    })
    let saveCount = 0

    ;(invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'list_ai_providers') {
        return Promise.resolve(providerListResponse)
      }
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
        saveCount += 1
        return saveCount === 1 ? firstSaveGate : Promise.resolve(undefined)
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

    const replacementsHeading = await screen.findByText('Corrections')
    const replacementsCard = replacementsHeading.parentElement?.parentElement
    expect(replacementsCard).toBeTruthy()
    const addRuleButton = within(replacementsCard as HTMLElement).getByRole('button', {
      name: /add/i,
    })

    await user.click(addRuleButton)
    await waitFor(() => expect(saveCount).toBe(1))
    await user.click(addRuleButton)
    rejectFirstSave?.()

    await waitFor(() => {
      expect(saveCount).toBe(2)
      expect(screen.getByText('Rule 2')).toBeInTheDocument()
    })
    expect(toast.error).not.toHaveBeenCalledWith('stale save failed')
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


  it('restores a remembered model when saving an API key for a different provider', async () => {
    aiSettingsResponse = {
      ...enabledAISettings,
      enabled: false,
      modelsByProvider: {
        openai: 'gpt-5-mini',
        gemini: 'gemini-1.5-flash',
      },
    }
    ;(hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(async (providerId: string) =>
      providerId === 'openai',
    )
    const user = userEvent.setup()
    renderWithProviders()

    const geminiHeading = await screen.findByText('Google Gemini')
    const geminiCard = geminiHeading.closest('.p-4')
    const openAIHeading = await screen.findByText('OpenAI')
    const openAICard = openAIHeading.closest('.p-4')
    expect(openAICard).toBeTruthy()
    expect(geminiCard).toBeTruthy()

    await user.click(within(geminiCard as HTMLElement).getByRole('button', { name: /add key/i }))
    await user.type(await screen.findByLabelText('API Key'), 'gemini-key')
    await user.click(screen.getByRole('button', { name: 'Save API Key' }))

    await waitFor(() => {
      expect(saveApiKey).toHaveBeenCalledWith('gemini', 'gemini-key')
      expect(within(geminiCard as HTMLElement).getByRole('button', { name: /gemini 1.5 flash/i })).toBeInTheDocument()
    })
    expect(within(openAICard as HTMLElement).getByRole('button', { name: /gpt-5 mini/i })).toBeInTheDocument()
  })

  it('keeps the selected model when saving an API key for the current provider', async () => {
    aiSettingsResponse = { ...enabledAISettings, enabled: false, hasApiKey: false }
    ;(hasApiKey as ReturnType<typeof vi.fn>).mockResolvedValue(false)
    const user = userEvent.setup()
    renderWithProviders()

    const openAIHeading = await screen.findByText('OpenAI')
    const openAICard = openAIHeading.closest('.p-4')
    expect(openAICard).toBeTruthy()

    await user.click(within(openAICard as HTMLElement).getByRole('button', { name: /add key/i }))
    await user.type(await screen.findByLabelText('API Key'), 'openai-key')
    await user.click(screen.getByRole('button', { name: 'Save API Key' }))

    await waitFor(() => {
      expect(saveApiKey).toHaveBeenCalledWith('openai', 'openai-key')
      expect(within(openAICard as HTMLElement).getByRole('button', { name: /gpt-5 mini/i })).toBeInTheDocument()
    })
  })
  it('surfaces migrated invalid AI model reselection and clears it when a model is selected', async () => {
    aiSettingsResponse = {
      ...enabledAISettings,
      model: '',
      modelsByProvider: {},
      aiModelNeedsReselection: true,
    }
    ;(hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(async (providerId: string) =>
      providerId === 'openai',
    )
    const user = userEvent.setup()
    renderWithProviders()

    expect(
      await screen.findByText(
        'Your previously selected AI model is no longer available. Please choose a model to continue using AI polish.',
      ),
    ).toBeInTheDocument()

    const openAIHeading = await screen.findByText('OpenAI')
    const openAICard = openAIHeading.closest('.p-4')
    expect(openAICard).toBeTruthy()

    await user.click(within(openAICard as HTMLElement).getByRole('button', { name: /gpt-5 mini/i }))

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_ai_settings', {
        enabled: true,
        provider: 'openai',
        model: 'gpt-5-mini',
      })
      expect(aiSettingsResponse.aiModelNeedsReselection).toBe(false)
      expect(
        screen.queryByText(/previously selected AI model is no longer available/i),
      ).not.toBeInTheDocument()
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
