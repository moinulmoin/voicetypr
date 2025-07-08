import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { HotkeyInput } from './HotkeyInput';

// Mock navigator.userAgent for consistent tests
beforeEach(() => {
  Object.defineProperty(navigator, 'userAgent', {
    value: 'Mac OS X',
    writable: true,
  });
});

describe('HotkeyInput', () => {
  const mockOnChange = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should render with initial value', () => {
    render(
      <HotkeyInput 
        value="CommandOrControl+Shift+Space" 
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />
    );

    const input = screen.getByRole('textbox') as HTMLInputElement;
    expect(input.value).toBe('⌘+⇧+␣');
  });

  it('should display placeholder when no value', () => {
    render(
      <HotkeyInput 
        value="" 
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />
    );

    const input = screen.getByPlaceholderText('Click to set hotkey');
    expect(input).toBeInTheDocument();
    expect(input.value).toBe('');
  });

  it('should enter recording mode on Edit click', async () => {
    const user = userEvent.setup();
    
    render(
      <HotkeyInput 
        value="CommandOrControl+Space" 
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />
    );

    // Find the edit button by its title
    const editButton = screen.getByTitle('Change hotkey');
    await user.click(editButton);

    // Check if we're in edit mode by looking for the save button
    expect(screen.getByTitle('Save hotkey')).toBeInTheDocument();
    expect(screen.getByTitle('Cancel')).toBeInTheDocument();
  });

  it('should save valid hotkey on Save click', async () => {
    const user = userEvent.setup();
    
    render(
      <HotkeyInput 
        value="" 
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />
    );

    // Click the edit button to enter edit mode
    const editButton = screen.getByTitle('Change hotkey');
    await user.click(editButton);

    // Now we should be in edit mode
    const input = screen.getByRole('textbox');
    
    // Click the input to start recording
    fireEvent.click(input);

    // Record valid combination
    fireEvent.keyDown(document, { key: 'Control', ctrlKey: true });
    fireEvent.keyDown(document, { key: 'A', ctrlKey: true });

    const saveButton = screen.getByTitle('Save hotkey');
    await user.click(saveButton);

    expect(mockOnChange).toHaveBeenCalledWith('CommandOrControl+A');
  });

  it('should cancel recording on Cancel click', async () => {
    const user = userEvent.setup();
    
    render(
      <HotkeyInput 
        value="CommandOrControl+Space" 
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />
    );

    const editButton = screen.getByTitle('Change hotkey');
    await user.click(editButton);

    const cancelButton = screen.getByTitle('Cancel');
    await user.click(cancelButton);

    // Should be back in display mode
    expect(screen.getByTitle('Change hotkey')).toBeInTheDocument();
    expect(mockOnChange).not.toHaveBeenCalled();
  });

  it('should validate key combinations', async () => {
    const user = userEvent.setup();
    
    render(
      <HotkeyInput 
        value="" 
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />
    );

    // Click the edit button to enter edit mode
    const editButton = screen.getByTitle('Change hotkey');
    await user.click(editButton);

    // Click the input to start recording
    const input = screen.getByRole('textbox');
    fireEvent.click(input);

    // Try with only one key - use keydown and keyup
    fireEvent.keyDown(document, { key: 'A' });
    fireEvent.keyUp(document, { key: 'A' });

    // Should show validation error
    await waitFor(() => {
      expect(screen.getByText(/Minimum 2 keys required/)).toBeInTheDocument();
    });
  });

  it('should handle Escape key to cancel', async () => {
    const user = userEvent.setup();
    
    render(
      <HotkeyInput 
        value="CommandOrControl+Space" 
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />
    );

    // Enter edit mode
    const editButton = screen.getByTitle('Change hotkey');
    await user.click(editButton);

    // Should now be in edit mode with Cancel button
    expect(screen.getByTitle('Cancel')).toBeInTheDocument();

    // Click input to start recording
    const input = screen.getByRole('textbox');
    fireEvent.click(input);

    // Press Escape
    fireEvent.keyDown(document, { key: 'Escape' });

    // Check that we're still in edit mode but not recording
    // The component stays in edit mode after Escape, but stops recording
    expect(screen.getByTitle('Cancel')).toBeInTheDocument();
    expect(mockOnChange).not.toHaveBeenCalled();
    
    // Can also test clicking Cancel to return to display mode
    const cancelButton = screen.getByTitle('Cancel');
    await user.click(cancelButton);
    
    // Now should be back in display mode
    await waitFor(() => {
      expect(screen.getByTitle('Change hotkey')).toBeInTheDocument();
    });
  });

  it('should disable save button for invalid combinations', async () => {
    const user = userEvent.setup();
    
    render(
      <HotkeyInput 
        value="" 
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />
    );

    // Enter edit mode
    const editButton = screen.getByTitle('Change hotkey');
    await user.click(editButton);

    // Click input to start recording
    const input = screen.getByRole('textbox');
    fireEvent.click(input);

    // Record only one key
    fireEvent.keyDown(document, { key: 'A' });
    fireEvent.keyUp(document, { key: 'A' });

    await waitFor(() => {
      const saveButton = screen.getByTitle('Save hotkey');
      expect(saveButton).toBeDisabled();
    });
  });

  it('should format shortcuts correctly for Windows', () => {
    // Mock Windows platform
    Object.defineProperty(navigator, 'userAgent', {
      value: 'Windows',
      writable: true,
    });

    render(
      <HotkeyInput 
        value="CommandOrControl+Shift+Space" 
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />
    );

    const input = screen.getByRole('textbox') as HTMLInputElement;
    expect(input.value).toBe('Ctrl+⇧+␣');
  });
});