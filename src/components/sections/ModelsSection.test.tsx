import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ModelsSection } from './ModelsSection';
import type { LocalModelInfo } from '@/types';

vi.mock('@/contexts/SettingsContext', () => ({
  useSettings: () => ({
    settings: {
      current_model: '',
      current_model_engine: 'whisper',
      language: 'en',
    },
    updateSettings: vi.fn().mockResolvedValue(undefined),
  }),
}));

vi.mock('@/components/LanguageSelection', () => ({
  LanguageSelection: () => <div data-testid="language-selection" />,
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

const noop = vi.fn();

function whisperModel(overrides: Partial<LocalModelInfo> = {}): LocalModelInfo {
  return {
    name: 'base.en',
    display_name: 'Base (English)',
    engine: 'whisper',
    kind: 'local',
    downloaded: false,
    requires_setup: false,
    size: 147_964_211,
    url: 'https://example.com/base.en.bin',
    sha256: '137c40403d78fd54d454da0f9bd998f78703390c',
    speed_score: 8,
    accuracy_score: 5,
    recommended: false,
    ...overrides,
  };
}

function renderModelsSection(props: Partial<Parameters<typeof ModelsSection>[0]> = {}) {
  return render(
    <ModelsSection
      models={[]}
      downloadProgress={{}}
      verifyingModels={new Set()}
      onDownload={noop}
      onDelete={noop}
      onCancelDownload={noop}
      onSelect={noop}
      refreshModels={async () => undefined}
      {...props}
    />
  );
}

describe('ModelsSection', () => {
  it('does not show a global downloading status in the header', () => {
    renderModelsSection({
      models: [['base.en', whisperModel()]],
      downloadProgress: { 'base.en': 12 },
    });

    expect(screen.queryByText('Downloading...')).not.toBeInTheDocument();
    expect(screen.getByText('12%')).toBeInTheDocument();
  });

  it('shows an initial loading state instead of an empty model-list message', () => {
    renderModelsSection({ isLoading: true });

    expect(screen.getByText('Loading models...')).toBeInTheDocument();
    expect(screen.queryByText('No models available')).not.toBeInTheDocument();
  });
});
