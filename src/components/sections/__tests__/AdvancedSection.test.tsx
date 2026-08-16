import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { AdvancedSection } from '../AdvancedSection';

const platformMock = vi.hoisted(() => ({ isMacOS: false }));

vi.mock('@/lib/platform', () => platformMock);

vi.mock('@/contexts/ReadinessContext', () => ({
  useReadiness: () => ({
    hasAccessibilityPermission: true,
    hasMicrophonePermission: true,
    isLoading: false,
    requestAccessibilityPermission: vi.fn(),
    requestMicrophonePermission: vi.fn(),
    checkAccessibilityPermission: vi.fn(),
    checkMicrophonePermission: vi.fn(),
  }),
}));

vi.mock('../TelemetrySection', () => ({
  TelemetrySection: () => <div data-testid="telemetry-section" />,
}));

describe('AdvancedSection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    platformMock.isMacOS = false;
  });

  it('shows the Quick fixes card on Diagnostics', () => {
    render(<AdvancedSection />);

    expect(screen.getByRole('heading', { name: /Diagnostics/ })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Quick fixes' })).toBeInTheDocument();
    expect(
      screen.getByText('Common issues you can check yourself before reporting.'),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /recording not working/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /shortcut not responding/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /text not inserting/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /model download stuck/i })).toBeInTheDocument();
  });

  it('shows Windows-specific quick-fix solutions', async () => {
    const user = userEvent.setup();
    render(<AdvancedSection />);

    await user.click(screen.getByRole('button', { name: /shortcut not responding/i }));

    expect(
      screen.getByText(
        'Open Shortcuts, and choose another shortcut if the current one is reserved by another app.',
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText(/grant Accessibility permission/i)).not.toBeInTheDocument();
  });

  it('shows macOS permission guidance only on macOS', async () => {
    platformMock.isMacOS = true;
    const user = userEvent.setup();
    render(<AdvancedSection />);

    await user.click(screen.getByRole('button', { name: /shortcut not responding/i }));

    expect(screen.getByText(/grant Accessibility permission/i)).toBeInTheDocument();
  });
});
