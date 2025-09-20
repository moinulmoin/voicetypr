import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { HotkeyInput } from './HotkeyInput';

// Mock the platform detection
vi.mock('@/lib/platform', () => ({
  isMacOS: true,
  isWindows: false,
  isLinux: false
}));

describe('HotkeyInput', () => {
  const mockOnChange = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('displays the current hotkey value', () => {
    render(
      <HotkeyInput 
        value="CommandOrControl+Shift+Space" 
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />
    );

    // User should see the hotkey displayed with platform-specific symbols
    expect(screen.getByText('⌘')).toBeInTheDocument();
    expect(screen.getByText('⇧')).toBeInTheDocument();
    expect(screen.getByText('Space')).toBeInTheDocument();
  });

  it('shows placeholder when no hotkey is set', () => {
    render(
      <HotkeyInput 
        value="" 
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />
    );

    expect(screen.getByText('Click to set hotkey')).toBeInTheDocument();
  });

  it('allows user to edit the hotkey', async () => {
    const user = userEvent.setup();
    
    render(
      <HotkeyInput 
        value="CommandOrControl+Space" 
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />
    );

    // User clicks the edit button
    const editButton = screen.getByTitle('Change hotkey');
    await user.click(editButton);

    // User should see they can now set a new hotkey
    expect(screen.getByText('Press keys to set hotkey')).toBeInTheDocument();
    expect(screen.getByTitle('Save hotkey')).toBeInTheDocument();
    expect(screen.getByTitle('Cancel')).toBeInTheDocument();
  });

  it('allows user to cancel editing', async () => {
    const user = userEvent.setup();
    
    render(
      <HotkeyInput 
        value="CommandOrControl+Space" 
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />
    );

    // User starts editing
    const editButton = screen.getByTitle('Change hotkey');
    await user.click(editButton);

    // User decides to cancel
    const cancelButton = screen.getByTitle('Cancel');
    await user.click(cancelButton);

    // Should return to showing the original hotkey
    expect(screen.getByText('⌘')).toBeInTheDocument();
    expect(screen.getByText('Space')).toBeInTheDocument();
    
    // Should not have changed the value
    expect(mockOnChange).not.toHaveBeenCalled();
  });

  it('prevents saving invalid hotkey combinations', async () => {
    const user = userEvent.setup();
    
    render(
      <HotkeyInput 
        value="" 
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />
    );

    // User starts editing
    const editButton = screen.getByTitle('Change hotkey');
    await user.click(editButton);

    // User presses just one key (invalid combination)
    await user.keyboard('a');

    // Save button should be disabled and error shown
    await waitFor(() => {
      const saveButton = screen.getByTitle('Save hotkey');
      expect(saveButton).toBeDisabled();
      expect(screen.getByText(/Minimum 2 key\(s\) required/)).toBeInTheDocument();
    });
  });

  it('saves valid hotkey combinations', async () => {
    const user = userEvent.setup();
    
    render(
      <HotkeyInput 
        value="" 
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />
    );

    // User starts editing
    const editButton = screen.getByTitle('Change hotkey');
    await user.click(editButton);

    // User presses a valid combination (Cmd+A on Mac)
    await user.keyboard('{Meta>}a{/Meta}');

    // User can save the hotkey
    await waitFor(() => {
      const saveButton = screen.getByTitle('Save hotkey');
      expect(saveButton).not.toBeDisabled();
    });

    const saveButton = screen.getByTitle('Save hotkey');
    await user.click(saveButton);

    // Should have called onChange with a normalized value
    await waitFor(() => {
      expect(mockOnChange).toHaveBeenCalled();
    });
  });

  it('displays platform-specific symbols correctly', () => {
    render(
      <HotkeyInput 
        value="CommandOrControl+Alt+Delete" 
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />
    );

    // On Mac, should show Mac symbols
    expect(screen.getByText('⌘')).toBeInTheDocument();  // Command
    expect(screen.getByText('⌥')).toBeInTheDocument();  // Option/Alt
    expect(screen.getByText('⌦')).toBeInTheDocument();  // Delete
  });

  it('allows Escape key to exit edit mode', async () => {
    const user = userEvent.setup();
    
    render(
      <HotkeyInput 
        value="CommandOrControl+Space" 
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />
    );

    // User starts editing
    const editButton = screen.getByTitle('Change hotkey');
    await user.click(editButton);

    // User presses Escape to cancel
    await user.keyboard('{Escape}');

    // Should return to display mode
    await waitFor(() => {
      expect(screen.getByTitle('Change hotkey')).toBeInTheDocument();
      expect(screen.getByText('⌘')).toBeInTheDocument();
    });
    
    expect(mockOnChange).not.toHaveBeenCalled();
  });
});