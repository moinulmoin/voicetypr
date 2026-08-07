import { act, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { emitMockEvent } from '@/test/setup';
import { RecordingPill } from './RecordingPill';

const { audioBarsMock, mockRecording, mockSettings } = vi.hoisted(() => ({
  audioBarsMock: vi.fn(),
  mockRecording: { state: 'idle' },
  mockSettings: {
    pill_indicator_mode: 'when_recording',
    pill_indicator_style: 'compact',
    pill_indicator_offset: 10,
  } as Record<string, unknown>,
}));

vi.mock('@/components/AudioBars', () => ({
  AudioBars: (props: { audioLevel?: number; state: string }) => {
    audioBarsMock(props);
    return (
      <div
        data-audio-level={props.audioLevel}
        data-state={props.state}
        data-testid="audio-bars"
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
    audioBarsMock.mockClear();
    mockRecording.state = 'idle';
    mockSettings.pill_indicator_mode = 'when_recording';
    mockSettings.pill_indicator_style = 'compact';
  });

  it('hides the pill when mode is never', () => {
    mockSettings.pill_indicator_mode = 'never';
    render(<RecordingPill />);
    expect(screen.queryByTestId('audio-bars')).not.toBeInTheDocument();
  });

  it('hides the pill when idle and mode is when_recording', () => {
    mockSettings.pill_indicator_mode = 'when_recording';
    mockRecording.state = 'idle';
    render(<RecordingPill />);
    expect(screen.queryByTestId('audio-bars')).not.toBeInTheDocument();
  });

  it('keeps the compact pill icon-only while recording', () => {
    mockSettings.pill_indicator_mode = 'when_recording';
    mockRecording.state = 'recording';
    render(<RecordingPill />);
    expect(screen.getByTestId('audio-bars')).toHaveAttribute('data-state', 'listening');
    expect(screen.queryByText('Listening')).not.toBeInTheDocument();
    expect(screen.queryByText('0:00')).not.toBeInTheDocument();
  });


  it('adds an elapsed timer only in the full listening style', () => {
    mockSettings.pill_indicator_style = 'full';
    mockRecording.state = 'recording';
    render(<RecordingPill />);

    expect(screen.getByText('Listening')).toBeInTheDocument();
    expect(screen.getByText('0:00')).toBeInTheDocument();
  });

  it('resets the elapsed timer between recording sessions', () => {
    vi.useFakeTimers();
    mockSettings.pill_indicator_style = 'full';
    mockRecording.state = 'recording';

    const { rerender } = render(<RecordingPill />);
    act(() => {
      vi.advanceTimersByTime(12_000);
    });
    expect(screen.getByText('0:12')).toBeInTheDocument();

    mockRecording.state = 'transcribing';
    rerender(<RecordingPill />);
    mockRecording.state = 'recording';
    rerender(<RecordingPill />);

    expect(screen.getByText('0:00')).toBeInTheDocument();
    vi.useRealTimers();
  });

  it('shows the pill when idle and mode is always', () => {
    mockSettings.pill_indicator_mode = 'always';
    mockRecording.state = 'idle';
    render(<RecordingPill />);
    expect(screen.getByTestId('audio-bars')).toHaveAttribute('data-state', 'idle');
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });

  it('shows status text for non-listening states only in the full style', () => {
    mockSettings.pill_indicator_mode = 'always';
    mockSettings.pill_indicator_style = 'full';
    mockRecording.state = 'transcribing';
    const { rerender } = render(<RecordingPill />);
    expect(screen.getByTestId('audio-bars')).toHaveAttribute('data-state', 'transcribing');
    expect(screen.getByText('Transcribing...')).toBeInTheDocument();

    mockRecording.state = 'stopping';
    rerender(<RecordingPill />);
    expect(screen.getByTestId('audio-bars')).toHaveAttribute('data-state', 'transcribing');
    expect(screen.getByText('Transcribing...')).toBeInTheDocument();
  });

  it('gives formatting feedback precedence over recording and transcribing states', () => {
    mockSettings.pill_indicator_mode = 'always';
    mockSettings.pill_indicator_style = 'full';
    mockRecording.state = 'recording';
    const { rerender } = render(<RecordingPill />);

    act(() => {
      emitMockEvent('enhancing-started', undefined);
    });
    expect(screen.getByTestId('audio-bars')).toHaveAttribute('data-state', 'formatting');
    expect(screen.getByRole('status')).toHaveTextContent('Polishing...');

    mockRecording.state = 'transcribing';
    rerender(<RecordingPill />);
    expect(screen.getByTestId('audio-bars')).toHaveAttribute('data-state', 'formatting');
    expect(screen.getByRole('status')).toHaveTextContent('Polishing...');

    act(() => {
      emitMockEvent('enhancing-completed', undefined);
    });
    expect(screen.getByTestId('audio-bars')).toHaveAttribute('data-state', 'transcribing');
    expect(screen.getByRole('status')).toHaveTextContent('Transcribing...');
  });
  it('clears Polishing and returns to the transcribing state when enhancing fails', () => {
    mockSettings.pill_indicator_mode = 'always';
    mockSettings.pill_indicator_style = 'full';
    mockRecording.state = 'transcribing';
    render(<RecordingPill />);

    act(() => {
      emitMockEvent('enhancing-started', undefined);
    });
    expect(screen.getByTestId('audio-bars')).toHaveAttribute('data-state', 'formatting');
    expect(screen.getByRole('status')).toHaveTextContent('Polishing...');

    act(() => {
      emitMockEvent('enhancing-failed', undefined);
    });
    // A failed enhancement must clear the formatting flag so the pill falls
    // back to the controller-derived prior state (transcribing), not stay
    // stuck showing Polishing.
    expect(screen.getByTestId('audio-bars')).toHaveAttribute('data-state', 'transcribing');
    expect(screen.getByRole('status')).toHaveTextContent('Transcribing...');
  });

  it('passes through audio levels while listening', () => {
    mockRecording.state = 'recording';
    render(<RecordingPill />);

    act(() => {
      emitMockEvent('audio-level', 0.42);
    });

    expect(screen.getByTestId('audio-bars')).toHaveAttribute('data-audio-level', '0.42');
  });
});
