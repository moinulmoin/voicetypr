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

const readinessState = vi.hoisted(() => ({
  value: null as { ai_ready: boolean } | null,
}))

vi.mock('@/contexts/ReadinessContext', () => ({
  useReadinessState: () => readinessState.value,
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
    sourceProvider?: string | null
    cliDefault?: boolean
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
    'claude-code': [
      { id: 'haiku', name: 'Haiku', recommended: true },
      { id: 'sonnet', name: 'Sonnet', recommended: false },
      { id: 'opus', name: 'Opus', recommended: false },
    ],
    pi: [
      { id: '', name: 'CLI default', recommended: true, cliDefault: true },
      {
        id: 'openai/gpt-5-mini',
        name: 'GPT-5 Mini',
        recommended: false,
        sourceProvider: 'OpenAI',
      },
      {
        id: 'anthropic/claude-sonnet-4',
        name: 'Claude Sonnet 4',
        recommended: false,
        sourceProvider: 'Anthropic',
      },
    ],
    omp: [
      { id: '', name: 'CLI default', recommended: true, cliDefault: true },
      {
        id: 'google/gemini-2.5-flash',
        name: 'Gemini 2.5 Flash',
        recommended: false,
        sourceProvider: 'Google',
      },
    ],
  }),
)
const modelDiscovery = vi.hoisted(() => ({
  loading: {} as Record<string, boolean>,
  errors: {} as Record<string, string | null>,
  fetchModels: vi.fn((providerId: string) => Promise.resolve(providerModels[providerId] || [])),
}))

vi.mock('@/hooks/useProviderModels', () => ({
  useAllProviderModels: () => ({
    fetchModels: (providerId: string) => modelDiscovery.fetchModels(providerId),
    getModels: (providerId: string) => providerModels[providerId] || [],
    isLoading: (providerId: string) => modelDiscovery.loading[providerId] || false,
    getError: (providerId: string) => modelDiscovery.errors[providerId] || null,
    clearModels: (providerId: string) => {
      delete modelDiscovery.errors[providerId]
      delete modelDiscovery.loading[providerId]
    },
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
let aiSettingsResponse: typeof baseAISettings = baseAISettings

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
  { id: 'claude-code', name: 'Claude Code', status: 'production', supportsReasoning: false },
  { id: 'pi', name: 'pi', status: 'production', supportsReasoning: false },
  { id: 'omp', name: 'oh-my-pi', status: 'production', supportsReasoning: false },
]

let rejectWritingSettingsUpdate = false
let agentCliProbeResponse: {
  state?: 'ready' | 'not_authenticated' | 'missing' | 'unsafe_launcher' | 'incompatible'
  installed: boolean
  authed: boolean
  detail?: string
} = { state: 'missing', installed: false, authed: false }
let enhancementOptionsResponse = { preset: 'PersonalDictation' }

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

async function openAdvanced(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole('button', { name: /toggle advanced/i }))
}

function getAdvancedProvidersPanel() {
  const providersHeading = screen.getByText('Providers & Models')
  const providersPanel = providersHeading.closest('fieldset')
  expect(providersPanel).toBeTruthy()
  return providersPanel as HTMLElement
}

describe('EnhancementsSection', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    readinessState.value = null
    modelDiscovery.loading = {}
    modelDiscovery.errors = {}
    window.localStorage.clear()
    rejectWritingSettingsUpdate = false
    aiSettingsResponse = baseAISettings
    enhancementOptionsResponse = { preset: 'PersonalDictation' }
    agentCliProbeResponse = { state: 'missing', installed: false, authed: false }
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
        return Promise.resolve(enhancementOptionsResponse)
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
      if (cmd === 'probe_agent_cli') {
        return Promise.resolve(agentCliProbeResponse)
      }
      return Promise.resolve(undefined)
    })
  })

  it('renders available providers and experimental badges in Advanced', async () => {
    const user = userEvent.setup()
    renderWithProviders()
    await openAdvanced(user)
    const providersPanel = getAdvancedProvidersPanel()

    expect(within(providersPanel).getByRole('heading', { name: 'OpenAI' })).toBeInTheDocument()
    expect(within(providersPanel).getByRole('heading', { name: 'Google Gemini' })).toBeInTheDocument()
    expect(within(providersPanel).getByRole('heading', { name: 'Anthropic' })).toBeInTheDocument()
    expect(
      within(providersPanel).getByRole('heading', { name: 'Custom (OpenAI-compatible)' }),
    ).toBeInTheDocument()
    expect(within(providersPanel).getByRole('heading', { name: 'Groq' })).toBeInTheDocument()
    expect(within(providersPanel).getByText('Experimental')).toBeInTheDocument()
    expect(within(providersPanel).getByLabelText('Search providers and models')).toBeInTheDocument()
  })

  it('filters providers and grouped models by search text', async () => {
    ;(hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(async (providerId: string) =>
      providerId === 'openai' || providerId === 'groq',
    )
    const user = userEvent.setup()
    renderWithProviders()
    await openAdvanced(user)

    await user.type(await screen.findByLabelText('Search providers and models'), 'llama')
    const providersPanel = getAdvancedProvidersPanel()

    await waitFor(() => {
      expect(within(providersPanel).queryByRole('heading', { name: 'OpenAI' })).not.toBeInTheDocument()
      expect(within(providersPanel).getByRole('heading', { name: 'Groq' })).toBeInTheDocument()
      expect(
        within(providersPanel).getByRole('button', { name: /llama 3\.3 70b versatile/i }),
      ).toBeInTheDocument()
      expect(within(providersPanel).getByText('Recommended')).toBeInTheDocument()
    })
  })

  it('persists selected model from the grouped picker', async () => {
    ;(hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(async (providerId: string) =>
      providerId === 'openai',
    )
    const user = userEvent.setup()
    renderWithProviders()
    await openAdvanced(user)

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
    const user = userEvent.setup()
    renderWithProviders()

    await waitFor(() => {
      expect(screen.getAllByText('Polish').length).toBeGreaterThan(0)
      expect(screen.getByText('Static Rules')).toBeInTheDocument()
      expect(screen.getByRole('button', { name: /toggle advanced/i })).toBeInTheDocument()
      expect(screen.getByText('Corrections')).toBeInTheDocument()
      expect(screen.getByText('Words & Names')).toBeInTheDocument()
      expect(screen.getByText('Text Shortcuts')).toBeInTheDocument()
      expect(screen.queryByRole('button', { name: 'Personal Dictation' })).not.toBeInTheDocument()
    })

    await openAdvanced(user)

    await waitFor(() => {
      expect(screen.getByText('Providers & Models')).toBeInTheDocument()
      const providersPanel = getAdvancedProvidersPanel()
      expect(within(providersPanel).getByRole('heading', { name: 'OpenAI' })).toBeInTheDocument()
      expect(
        within(providersPanel).getByRole('heading', { name: 'Google Gemini' }),
      ).toBeInTheDocument()
      expect(invoke).toHaveBeenCalledWith('list_ai_providers')
    })
  })

  it('removes the global mode picker from the simple Polish surface', async () => {
    renderWithProviders()

    await waitFor(() => {
      expect(screen.getByText('Static Rules')).toBeInTheDocument()
    })
    expect(screen.queryByText('Formatting mode')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Writing' })).not.toBeInTheDocument()
  })

  it('shows the guided setup card when Polish is unconfigured', async () => {
    renderWithProviders()

    await waitFor(() => {
      expect(
        screen.getByText('Clean up grammar and punctuation while keeping your meaning.'),
      ).toBeInTheDocument()
      expect(
        screen.getByText('Connect an AI to turn on Polish'),
      ).toBeInTheDocument()
      expect(
        screen.getByText(
          'Polish uses a cloud AI you bring a key for. Pick one — setup takes about two minutes.',
        ),
      ).toBeInTheDocument()
      expect(screen.getByRole('button', { name: 'Anthropic' })).toBeInTheDocument()
      expect(screen.getByRole('button', { name: 'OpenAI' })).toBeInTheDocument()
      expect(screen.getByRole('button', { name: 'Google' })).toBeInTheDocument()
      expect(screen.getByText('Your key stays on this device.')).toBeInTheDocument()
      expect(screen.getByRole('switch', { name: /polish/i })).toBeDisabled()
    })
  })

  it('opens the existing API key modal from a guided provider button', async () => {
    const user = userEvent.setup()
    renderWithProviders()

    await user.click(await screen.findByRole('button', { name: 'Anthropic' }))

    await waitFor(() => {
      expect(screen.getByRole('dialog')).toBeInTheDocument()
      expect(screen.getByText('Add Anthropic API Key')).toBeInTheDocument()
      expect(screen.getByLabelText('API Key')).toBeInTheDocument()
    })
  })

  it('does NOT open the API key modal for a not-ready CLI provider (guides sign-in instead)', async () => {
    // Regression: CLI providers have no API key. Clicking a not-yet-ready CLI
    // provider in the guided card must NOT open the key modal — it guides the
    // user to install / sign in (toast), never the paste-a-key dialog.
    agentCliProbeResponse = { installed: true, authed: false }
    const user = userEvent.setup()
    renderWithProviders()

    await user.click(await screen.findByRole('button', { name: 'Claude Code' }))

    await waitFor(() => {
      expect(toast.info).toHaveBeenCalled()
    })
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
    expect(screen.queryByLabelText('API Key')).not.toBeInTheDocument()
  })

  it('auto-selects the recommended model and turns Polish on after guided key validation', async () => {
    const user = userEvent.setup()
    renderWithProviders()

    await user.click(await screen.findByRole('button', { name: 'OpenAI' }))
    await user.type(await screen.findByLabelText('API Key'), 'openai-key')
    await user.click(screen.getByRole('button', { name: 'Save API Key' }))

    await waitFor(() => {
      expect(saveApiKey).toHaveBeenCalledWith('openai', 'openai-key')
      expect(invoke).toHaveBeenCalledWith('update_ai_settings', {
        enabled: true,
        provider: 'openai',
        model: 'gpt-5-mini',
      })
      expect(invoke).toHaveBeenCalledWith('update_enhancement_options', {
        options: { preset: 'CleanDictation' },
      })
      expect(toast.success).toHaveBeenCalledWith('Polish on')
    })
  })

  it('keeps a loaded key-based provider connected when backend AI readiness is ready', async () => {
    readinessState.value = { ai_ready: true }
    aiSettingsResponse = {
      ...enabledAISettings,
      provider: 'anthropic',
      model: 'claude-sonnet-4',
      modelsByProvider: { anthropic: 'claude-sonnet-4' },
    }
    vi.mocked(hasApiKey).mockResolvedValue(false)

    renderWithProviders()

    await waitFor(() => {
      const polishSwitch = screen.getByRole('switch', { name: 'Polish' })
      expect(polishSwitch).toBeChecked()
      expect(polishSwitch).not.toBeDisabled()
      expect(
        screen.getAllByText((_, element) =>
          element?.textContent === 'Using Anthropic · Claude Sonnet 4 · Active · Change',
        ).length,
      ).toBeGreaterThan(0)
    })
  })

  it('does not promote a non-ready agent CLI from backend AI readiness', async () => {
    readinessState.value = { ai_ready: true }
    aiSettingsResponse = {
      ...enabledAISettings,
      provider: 'claude-code',
      model: 'haiku',
      modelsByProvider: { 'claude-code': 'haiku' },
    }
    agentCliProbeResponse = {
      state: 'not_authenticated',
      installed: true,
      authed: false,
    }
    vi.mocked(hasApiKey).mockResolvedValue(false)

    renderWithProviders()

    await waitFor(() => {
      const polishSwitch = screen.getByRole('switch', { name: 'Polish' })
      expect(polishSwitch).toBeDisabled()
      expect(screen.queryByText(/Using Claude Code/)).not.toBeInTheDocument()
    })
  })

  it('shows the connected status line when Polish is configured but off', async () => {
    aiSettingsResponse = { ...enabledAISettings, enabled: false }
    ;(hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(async (providerId: string) =>
      providerId === 'openai',
    )

    const user = userEvent.setup()
    renderWithProviders()

    await waitFor(() => {
      expect(
        screen.getAllByText((_, element) =>
          element?.textContent === 'Using OpenAI · GPT-5 Mini · Change',
        ).length,
      ).toBeGreaterThan(0)
      expect(screen.queryByText('Connect an AI to turn on Polish')).not.toBeInTheDocument()
    })

    await user.click(screen.getByRole('button', { name: /change polish provider or model/i }))

    await waitFor(() => {
      expect(screen.getByText('Providers & Models')).toBeInTheDocument()
    })
  })

  it('keeps the simple Polish surface free of paywall or locked cues', async () => {
    const { container } = renderWithProviders()

    await waitFor(() => {
      expect(screen.getByText('Connect an AI to turn on Polish')).toBeInTheDocument()
      expect(screen.getByText('Not set up yet')).toBeInTheDocument()
    })
    expect(container.querySelector('.lucide-lock')).toBeNull()
    expect(screen.queryByText(/premium|paywall|locked|requires Polish/i)).not.toBeInTheDocument()
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

  it('migrates reshaping presets to Clean Dictation when Polish is on', async () => {
    aiSettingsResponse = enabledAISettings
    enhancementOptionsResponse = { preset: 'Writing' }
    ;(hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(async (providerId: string) =>
      providerId === 'openai',
    )

    renderWithProviders()

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_enhancement_options', {
        options: { preset: 'CleanDictation' },
      })
      expect(toast.info).toHaveBeenCalledWith('Reshaping now lives in Advanced -> App Rules.')
    })
    expect(window.localStorage.getItem('polish_reshape_migration_notified')).toBe('true')
  })

  it('keeps the reshaping migration notice one-time while still migrating', async () => {
    window.localStorage.setItem('polish_reshape_migration_notified', 'true')
    aiSettingsResponse = enabledAISettings
    enhancementOptionsResponse = { preset: 'Notes' }
    ;(hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(async (providerId: string) =>
      providerId === 'openai',
    )

    renderWithProviders()

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_enhancement_options', {
        options: { preset: 'CleanDictation' },
      })
    })
    expect(toast.info).not.toHaveBeenCalled()
  })

  it('does not migrate reshaping presets while Polish is off', async () => {
    enhancementOptionsResponse = { preset: 'Writing' }
    renderWithProviders()

    await waitFor(() => {
      expect(screen.getByText('Static Rules')).toBeInTheDocument()
    })
    expect(invoke).not.toHaveBeenCalledWith('update_enhancement_options', {
      options: { preset: 'CleanDictation' },
    })
  })

  it('surfaces migration persistence failures without looping', async () => {
    aiSettingsResponse = enabledAISettings
    enhancementOptionsResponse = { preset: 'Writing' }
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
        return Promise.resolve({ preset: 'Writing' })
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

    renderWithProviders()

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_enhancement_options', {
        options: { preset: 'CleanDictation' },
      })
    })
    expect(toast.info).not.toHaveBeenCalled()
  })

  it('switches to Personal Dictation when Polish is turned off', async () => {
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

    const aiToggle = await screen.findByRole('switch', { name: /polish/i })
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
      expect(toast.success).toHaveBeenCalledWith('Polish off')
    })
  })

  it('switches to Clean Dictation when Polish is turned on from Personal Dictation', async () => {
    aiSettingsResponse = { ...enabledAISettings, enabled: false }
    ;(hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(async (providerId: string) =>
      providerId === 'openai',
    )
    const user = userEvent.setup()
    renderWithProviders()

    const aiToggle = await screen.findByRole('switch', { name: /polish/i })
    await waitFor(() => expect(aiToggle).toBeEnabled())
    await user.click(aiToggle)

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_enhancement_options', {
        options: { preset: 'CleanDictation' },
      })
    })
  })

  it('saves custom provider setup without enabling Polish', async () => {
    aiSettingsResponse = { ...baseAISettings, provider: '', model: '' }
    const user = userEvent.setup()
    renderWithProviders()
    await openAdvanced(user)

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
    enhancementOptionsResponse = { preset: 'CleanDictation' }
    ;(hasApiKey as ReturnType<typeof vi.fn>).mockImplementation(async (providerId: string) =>
      providerId === 'openai',
    )
    const user = userEvent.setup()
    renderWithProviders()

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

  it('renders the three tiers with their copy', async () => {
    renderWithProviders()

    await waitFor(() => {
      expect(screen.getAllByText('Polish').length).toBeGreaterThan(0)
      expect(
        screen.getByText(
          'Clean up grammar and punctuation while keeping your meaning.',
        ),
      ).toBeInTheDocument()
      expect(screen.getByText('Static Rules')).toBeInTheDocument()
      expect(
        screen.getByText(
          'Work with or without Polish — even better with it. They also sharpen recognition.',
        ),
      ).toBeInTheDocument()
      expect(screen.getByText('Provider and model setup, plus per-app reshaping.')).toBeInTheDocument()
    })
  })

  it('does not render a context_policy control after the app-hint removal', async () => {
    renderWithProviders()

    await waitFor(() => expect(screen.getByText('Static Rules')).toBeInTheDocument())
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
    await openAdvanced(user)

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
    await openAdvanced(user)

    const geminiHeading = await screen.findByRole('heading', { name: 'Google Gemini' })
    const geminiCard = geminiHeading.closest('.rounded-xl')
    const openAIHeading = await screen.findByRole('heading', { name: 'OpenAI' })
    const openAICard = openAIHeading.closest('.rounded-xl')
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
    await openAdvanced(user)

    const openAIHeading = await screen.findByRole('heading', { name: 'OpenAI' })
    const openAICard = openAIHeading.closest('.rounded-xl')
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
    await openAdvanced(user)

    expect(
      await screen.findByText(
        'Your previously selected AI model is no longer available. Please choose a model to continue using Polish.',
      ),
    ).toBeInTheDocument()

    const providersPanel = getAdvancedProvidersPanel()
    const openAICard = within(providersPanel)
      .getByRole('heading', { name: 'OpenAI' })
      .closest('div.rounded-xl') as HTMLElement
    await user.click(within(openAICard).getByRole('button', { name: /gpt-5 mini/i }))

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

  it('shows Polish setup guidance in the guide dialog', async () => {
    const user = userEvent.setup()
    renderWithProviders()

    await user.click(await screen.findByRole('button', { name: /polish guide/i }))

    await waitFor(() => {
      const dialog = screen.getByRole('dialog')
      expect(within(dialog).getByText(/Save a key and Polish chooses/i)).toBeInTheDocument()
      expect(within(dialog).getAllByText(/Static Rules/i).length).toBeGreaterThan(0)
      expect(toast.error).not.toHaveBeenCalled()
    })
  })

  it('shows a sign-in hint and Refresh for an installed-but-unauthed agent CLI', async () => {
    agentCliProbeResponse = { state: 'not_authenticated', installed: true, authed: false }
    const user = userEvent.setup()
    renderWithProviders()
    await openAdvanced(user)
    const providersPanel = getAdvancedProvidersPanel()
    const claudeCard = within(providersPanel)
      .getByRole('heading', { name: 'Claude Code' })
      .closest('div.rounded-xl') as HTMLElement
    expect(await within(claudeCard).findByText(/not signed in/i)).toBeInTheDocument()

    expect(within(claudeCard).queryByText(/Add API key/i)).not.toBeInTheDocument()
    const refresh = within(claudeCard).getByRole('button', {
      name: /refresh claude code sign-in/i,
    })
    expect(refresh).toBeInTheDocument()

    agentCliProbeResponse = { state: 'ready', installed: true, authed: true }
    await user.click(refresh)
    await waitFor(() => {
      expect(toast.success).toHaveBeenCalledWith('Claude Code: signed in')
    })
  })
  it('uses the ready Claude CLI picker with curated defaults', async () => {
    agentCliProbeResponse = { state: 'ready', installed: true, authed: true }
    const user = userEvent.setup()
    renderWithProviders()
    await openAdvanced(user)
    const providersPanel = getAdvancedProvidersPanel()
    const claudeCard = within(providersPanel)
      .getByRole('heading', { name: 'Claude Code' })
      .closest('div.rounded-xl') as HTMLElement

    expect(within(claudeCard).getByRole('button', { name: /haiku/i })).toBeInTheDocument()
    expect(within(claudeCard).getByRole('button', { name: /sonnet/i })).toBeInTheDocument()
    expect(within(claudeCard).getByRole('button', { name: /opus/i })).toBeInTheDocument()
    expect(within(claudeCard).getByText('Recommended')).toBeInTheDocument()

    await user.click(within(claudeCard).getByRole('button', { name: /sonnet/i }))
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_ai_settings', {
        enabled: false,
        provider: 'claude-code',
        model: 'sonnet',
      })
    })
  })

  it('guided Claude setup enables the recommended Haiku model', async () => {
    agentCliProbeResponse = { state: 'ready', installed: true, authed: true }
    const user = userEvent.setup()
    renderWithProviders()

    await user.click(await screen.findByRole('button', { name: 'Claude Code' }))
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_ai_settings', {
        enabled: true,
        provider: 'claude-code',
        model: 'haiku',
      })
      expect(toast.success).toHaveBeenCalledWith('Polish on')
    })
  })

  it('groups pi and oh-my-pi models by source and persists the CLI default as an empty id', async () => {
    agentCliProbeResponse = { state: 'ready', installed: true, authed: true }
    const user = userEvent.setup()
    renderWithProviders()
    await openAdvanced(user)
    const providersPanel = getAdvancedProvidersPanel()
    const piCard = within(providersPanel)
      .getByRole('heading', { name: 'pi' })
      .closest('div.rounded-xl') as HTMLElement
    const ompCard = within(providersPanel)
      .getByRole('heading', { name: 'oh-my-pi' })
      .closest('div.rounded-xl') as HTMLElement

    expect(within(piCard).getAllByText('CLI default')).toHaveLength(2)
    expect(within(piCard).getByText('OpenAI')).toBeInTheDocument()
    expect(within(piCard).getByText('Anthropic')).toBeInTheDocument()
    expect(within(ompCard).getAllByText('CLI default')).toHaveLength(2)
    expect(within(ompCard).getByText('Google')).toBeInTheDocument()
    expect(within(providersPanel).queryByRole('combobox', { name: /thinking|effort/i })).not.toBeInTheDocument()

    await user.click(within(piCard).getByRole('button', { name: /CLI default/i }))
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_ai_settings', {
        enabled: false,
        provider: 'pi',
        model: '',
      })
    })
  })

  it('shows discovery errors with Retry without API-key or no-model dead ends', async () => {
    agentCliProbeResponse = { state: 'ready', installed: true, authed: true }
    modelDiscovery.errors['claude-code'] = 'Model discovery failed'
    const user = userEvent.setup()
    renderWithProviders()
    await openAdvanced(user)
    const providersPanel = getAdvancedProvidersPanel()
    const claudeCard = within(providersPanel)
      .getByRole('heading', { name: 'Claude Code' })
      .closest('div.rounded-xl') as HTMLElement

    expect(within(claudeCard).getByText('Model discovery failed')).toBeInTheDocument()
    expect(within(claudeCard).getByRole('button', { name: 'Retry' })).toBeInTheDocument()
    expect(within(claudeCard).queryByText(/Add API key/i)).not.toBeInTheDocument()
    expect(within(claudeCard).queryByText(/No models available/i)).not.toBeInTheDocument()

    await user.click(within(claudeCard).getByRole('button', { name: 'Retry' }))
    expect(modelDiscovery.fetchModels).toHaveBeenCalledWith('claude-code')
  })
  it.each([
    ['missing', /CLI not found/i],
    ['unsafe_launcher', /could not be used safely/i],
    ['incompatible', /incompatible/i],
  ] as const)('keeps the %s CLI state actionable and refreshable', async (state, copy) => {
    agentCliProbeResponse = { state, installed: false, authed: false }
    const user = userEvent.setup()
    renderWithProviders()
    await openAdvanced(user)
    const providersPanel = getAdvancedProvidersPanel()
    const claudeCard = within(providersPanel)
      .getByRole('heading', { name: 'Claude Code' })
      .closest('div.rounded-xl') as HTMLElement

    expect(within(claudeCard).getByText(copy)).toBeInTheDocument()
    if (state === 'missing') {
      expect(
        within(claudeCard).getByText(/Install it in an existing PATH directory, then Refresh/i),
      ).toBeInTheDocument()
      expect(
        within(claudeCard).getByText(
          /Refresh can detect installs in existing PATH directories; restart only if PATH itself changed/i,
        ),
      ).toBeInTheDocument()
    }
    expect(
      within(claudeCard).getByRole('button', { name: /refresh claude code sign-in/i }),
    ).toBeInTheDocument()
    expect(within(claudeCard).queryByText(/Add API key/i)).not.toBeInTheDocument()
    expect(within(claudeCard).queryByText(/No models available/i)).not.toBeInTheDocument()
  })

  it('shows Signed in and the ready model picker for an authed agent CLI', async () => {
    agentCliProbeResponse = { state: 'ready', installed: true, authed: true }
    const user = userEvent.setup()
    renderWithProviders()
    await openAdvanced(user)
    const providersPanel = getAdvancedProvidersPanel()
    const claudeCard = within(providersPanel)
      .getByRole('heading', { name: 'Claude Code' })
      .closest('div.rounded-xl') as HTMLElement

    expect(await within(claudeCard).findByText('Signed in')).toBeInTheDocument()
    expect(await within(claudeCard).findByRole('button', { name: /haiku/i })).toBeInTheDocument()
    expect(within(providersPanel).queryByText(/No models available/i)).not.toBeInTheDocument()
    expect(within(providersPanel).queryByRole('combobox', { name: /thinking|effort/i })).not.toBeInTheDocument()
  })
})
