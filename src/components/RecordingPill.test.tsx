import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { RecordingPill } from './RecordingPill';

const mockRecording: { state: string } = { state: 'idle' };
const mockSettings: Record<string, unknown> = {
  pill_indicator_mode: 'when_recording',
  pill_indicator_offset: 10
};

vi.mock('@/components/AudioDots', () => ({
  AudioDots: () => <div data-testid="audio-dots" />
}));

vi.mock('@/hooks/useRecording', () => ({
  useRecording: () => mockRecording
}));

vi.mock('@/contexts/SettingsContext', () => ({
  useSetting: (key: string) => mockSettings[key]
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {})
}));

describe('RecordingPill', () => {
  beforeEach(() => {
    mockRecording.state = 'idle';
    mockSettings.pill_indicator_mode = 'when_recording';
  });

  it('hides the pill when mode is never', () => {
    mockSettings.pill_indicator_mode = 'never';
    render(<RecordingPill />);
    expect(screen.queryByTestId('audio-dots')).not.toBeInTheDocument();
  });

  it('hides the pill when idle and mode is when_recording', () => {
    mockSettings.pill_indicator_mode = 'when_recording';
    mockRecording.state = 'idle';
    render(<RecordingPill />);
    expect(screen.queryByTestId('audio-dots')).not.toBeInTheDocument();
  });

  it('shows the pill when recording and mode is when_recording', () => {
    mockSettings.pill_indicator_mode = 'when_recording';
    mockRecording.state = 'recording';
    render(<RecordingPill />);
    expect(screen.getByTestId('audio-dots')).toBeInTheDocument();
  });

  it('shows the pill when idle and mode is always', () => {
    mockSettings.pill_indicator_mode = 'always';
    mockRecording.state = 'idle';
    render(<RecordingPill />);
    expect(screen.getByTestId('audio-dots')).toBeInTheDocument();
  });
});
