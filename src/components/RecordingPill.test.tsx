import { act, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { emitMockEvent } from '@/test/setup';
import { RecordingPill } from './RecordingPill';

const { audioDotsMock, mockRecording, mockSettings } = vi.hoisted(() => ({
  audioDotsMock: vi.fn(),
  mockRecording: { state: 'idle' },
  mockSettings: {
    pill_indicator_mode: 'when_recording',
    pill_indicator_offset: 10,
  } as Record<string, unknown>,
}));

vi.mock('@/components/AudioDots', () => ({
  AudioDots: (props: { audioLevel?: number; state: string }) => {
    audioDotsMock(props);
    return (
      <div
        data-audio-level={props.audioLevel}
        data-state={props.state}
        data-testid="audio-dots"
      />
    );
  },
}));

vi.mock('@/hooks/useRecording', () => ({
  useRecording: () => mockRecording,
}));

vi.mock('@/contexts/SettingsContext', () => ({
  useSetting: (key: string) => mockSettings[key],
}));

describe('RecordingPill', () => {
  beforeEach(() => {
    audioDotsMock.mockClear();
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
    expect(screen.getByTestId('audio-dots')).toHaveAttribute('data-state', 'listening');
  });

  it('shows the pill when idle and mode is always', () => {
    mockSettings.pill_indicator_mode = 'always';
    mockRecording.state = 'idle';
    render(<RecordingPill />);
    expect(screen.getByTestId('audio-dots')).toHaveAttribute('data-state', 'idle');
  });

  it('maps transcribing and stopping backend states to transcribing dots', () => {
    mockSettings.pill_indicator_mode = 'always';
    mockRecording.state = 'transcribing';
    const { rerender } = render(<RecordingPill />);
    expect(screen.getByTestId('audio-dots')).toHaveAttribute('data-state', 'transcribing');

    mockRecording.state = 'stopping';
    rerender(<RecordingPill />);
    expect(screen.getByTestId('audio-dots')).toHaveAttribute('data-state', 'transcribing');
  });

  it('gives formatting feedback precedence over recording and transcribing states', () => {
    mockSettings.pill_indicator_mode = 'always';
    mockRecording.state = 'recording';
    const { rerender } = render(<RecordingPill />);

    act(() => {
      emitMockEvent('enhancing-started', undefined);
    });
    expect(screen.getByTestId('audio-dots')).toHaveAttribute('data-state', 'formatting');

    mockRecording.state = 'transcribing';
    rerender(<RecordingPill />);
    expect(screen.getByTestId('audio-dots')).toHaveAttribute('data-state', 'formatting');

    act(() => {
      emitMockEvent('enhancing-completed', undefined);
    });
    expect(screen.getByTestId('audio-dots')).toHaveAttribute('data-state', 'transcribing');
  });

  it('passes through audio levels while listening', () => {
    mockRecording.state = 'recording';
    render(<RecordingPill />);

    act(() => {
      emitMockEvent('audio-level', 0.42);
    });

    expect(screen.getByTestId('audio-dots')).toHaveAttribute('data-audio-level', '0.42');
  });
});
