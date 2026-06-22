import { render, screen, waitFor, fireEvent, act } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { AgentCliSection } from '../AgentCliSection';

const mockInvoke = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    warning: vi.fn(),
  },
}));

describe('AgentCliSection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockReset();
  });

  it('renders Install command when status is not installed but manageable', async () => {
    mockInvoke.mockResolvedValue({
      installed: false,
      manageable: true,
      path: null,
    });

    render(<AgentCliSection />);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('cli_tool_status');
    });

    await waitFor(() => {
      expect(
        screen.getByRole('button', { name: /install command/i }),
      ).toBeInTheDocument();
    });

    // Recipe is always shown.
    expect(screen.getByText(/voicetypr transcribe/)).toBeInTheDocument();
    expect(screen.getByText(/voicetypr record --json/)).toBeInTheDocument();
    expect(screen.getByText(/voicetypr --help/)).toBeInTheDocument();
  });

  it('calls install_cli_tool on click and reflects the installed/remove state', async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'cli_tool_status') {
        return Promise.resolve({
          installed: false,
          manageable: true,
          path: null,
        });
      }
      if (command === 'install_cli_tool') {
        return Promise.resolve({
          installed: true,
          manageable: true,
          path: '/usr/local/bin/voicetypr',
        });
      }
      return Promise.resolve({
        installed: false,
        manageable: true,
        path: null,
      });
    });

    render(<AgentCliSection />);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('cli_tool_status');
    });

    const installBtn = await screen.findByRole('button', {
      name: /install command/i,
    });

    await act(async () => {
      fireEvent.click(installBtn);
    });

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('install_cli_tool');
    });

    // After install resolves with installed:true, the remove state is shown.
    await waitFor(() => {
      expect(
        screen.getByRole('button', { name: /remove command/i }),
      ).toBeInTheDocument();
    });
    expect(
      screen.queryByRole('button', { name: /install command/i }),
    ).not.toBeInTheDocument();
  });

  it('shows the recipe but no install button when not manageable', async () => {
    mockInvoke.mockResolvedValue({
      installed: false,
      manageable: false,
      path: null,
    });

    render(<AgentCliSection />);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('cli_tool_status');
    });

    await waitFor(() => {
      expect(
        screen.queryByRole('button', { name: /install command/i }),
      ).not.toBeInTheDocument();
    });

    expect(screen.getByText(/voicetypr transcribe/)).toBeInTheDocument();
    expect(screen.getByText(/voicetypr record --json/)).toBeInTheDocument();
    expect(screen.getByText(/voicetypr --help/)).toBeInTheDocument();
  });
});
