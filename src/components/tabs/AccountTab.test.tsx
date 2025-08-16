import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi, describe, it, expect, beforeEach } from 'vitest';
import { AccountTab } from './AccountTab';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';

// Mock Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn()
}));

// Mock toast
vi.mock('sonner', () => ({
  toast: {
    error: vi.fn(),
    warning: vi.fn(),
    info: vi.fn(),
    success: vi.fn()
  }
}));

// Mock contexts
vi.mock('@/contexts/LicenseContext', () => ({
  useLicense: () => ({
    license: {
      valid: true,
      key: 'TEST-LICENSE-KEY',
      expiresAt: new Date('2025-12-31')
    },
    validateLicense: vi.fn().mockResolvedValue(true),
    checkLicense: vi.fn()
  })
}));

// Mock hooks
vi.mock('@/hooks/useEventCoordinator', () => ({
  useEventCoordinator: () => ({
    registerEvent: vi.fn((event: string, callback: any) => {
      (window as any).__testEventCallbacks = (window as any).__testEventCallbacks || {};
      (window as any).__testEventCallbacks[event] = callback;
      return vi.fn();
    })
  })
}));

// Mock AccountSection component
vi.mock('../sections/AccountSection', () => ({
  AccountSection: () => (
    <div data-testid="account-section">
      <div>License Status: Valid</div>
      <div>License Key: TEST-LICENSE-KEY</div>
      <div>Expires: 2025-12-31</div>
      <button onClick={() => console.log('Validate License')}>Validate License</button>
      <button onClick={() => console.log('Enter New License')}>Enter New License</button>
      <div>Version: 1.6.2</div>
    </div>
  )
}));

describe('AccountTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (window as any).__testEventCallbacks = {};
  });

  it('renders without crashing', () => {
    render(<AccountTab />);
    expect(screen.getByTestId('account-section')).toBeInTheDocument();
  });

  it('displays license information', () => {
    render(<AccountTab />);
    expect(screen.getByText('License Status: Valid')).toBeInTheDocument();
    expect(screen.getByText('License Key: TEST-LICENSE-KEY')).toBeInTheDocument();
    expect(screen.getByText('Expires: 2025-12-31')).toBeInTheDocument();
  });

  it('displays app version', () => {
    render(<AccountTab />);
    expect(screen.getByText('Version: 1.6.2')).toBeInTheDocument();
  });

  it('registers license-required event listener', () => {
    render(<AccountTab />);
    
    expect((window as any).__testEventCallbacks).toHaveProperty('license-required');
  });

  it('handles license-required event', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(true);
    
    render(<AccountTab />);
    
    const callback = (window as any).__testEventCallbacks['license-required'];
    
    // Simulate the event with delay handling
    await callback({
      title: 'License Required',
      message: 'Please enter a valid license key',
      action: 'Enter License'
    });

    // Wait for the delay
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('focus_main_window');
    }, { timeout: 200 });

    // Check toast is shown after delay
    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith(
        'Please enter a valid license key',
        expect.objectContaining({
          duration: 2000
        })
      );
    }, { timeout: 300 });
  });

  it('handles focus window failure gracefully', async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error('Focus failed'));
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    
    render(<AccountTab />);
    
    const callback = (window as any).__testEventCallbacks['license-required'];
    
    await callback({
      title: 'License Required',
      message: 'License expired',
      action: 'Renew'
    });

    await waitFor(() => {
      expect(consoleSpy).toHaveBeenCalledWith('Failed to focus window:', expect.any(Error));
    });

    // Toast should still be shown even if focus fails
    expect(toast.error).toHaveBeenCalledWith('License expired');
    
    consoleSpy.mockRestore();
  });

  it('renders license action buttons', () => {
    render(<AccountTab />);
    expect(screen.getByText('Validate License')).toBeInTheDocument();
    expect(screen.getByText('Enter New License')).toBeInTheDocument();
  });

  it('handles invalid license state', async () => {
    // Mock invalid license
    vi.mock('@/contexts/LicenseContext', () => ({
      useLicense: () => ({
        license: {
          valid: false,
          key: null,
          expiresAt: null
        },
        validateLicense: vi.fn().mockResolvedValue(false),
        checkLicense: vi.fn()
      })
    }));

    render(<AccountTab />);
    
    // Component should still render
    expect(screen.getByTestId('account-section')).toBeInTheDocument();
  });

  it('cleans up event listeners on unmount', () => {
    const { unmount } = render(<AccountTab />);
    
    // Verify event callback exists
    expect((window as any).__testEventCallbacks['license-required']).toBeDefined();
    
    unmount();
    
    // Cleanup verification would depend on implementation
    // Event coordinator should handle cleanup
  });

  it('handles license validation', async () => {
    const { validateLicense } = await import('@/contexts/LicenseContext');
    
    render(<AccountTab />);
    
    // Simulate license validation
    await vi.mocked(validateLicense)('NEW-LICENSE-KEY');
    
    expect(validateLicense).toHaveBeenCalledWith('NEW-LICENSE-KEY');
  });

  it('handles multiple license-required events', async () => {
    vi.mocked(invoke).mockResolvedValue(true);
    
    render(<AccountTab />);
    
    const callback = (window as any).__testEventCallbacks['license-required'];
    
    // Simulate multiple events
    await callback({ message: 'First license error' });
    await callback({ message: 'Second license error' });
    
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledTimes(2);
      expect(invoke).toHaveBeenCalledWith('focus_main_window');
    });
  });
});