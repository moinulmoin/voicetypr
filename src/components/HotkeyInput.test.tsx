import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
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

  it('should render with initial value showing kbd elements', () => {
    render(
      <HotkeyInput 
        value="CommandOrControl+Shift+Space" 
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />
    );

    // Check for the kbd elements displaying the hotkey
    expect(screen.getByText('⌘')).toBeInTheDocument();
    expect(screen.getByText('⇧')).toBeInTheDocument();
    expect(screen.getByText('Space')).toBeInTheDocument();
    
    // Check for the edit button
    expect(screen.getByTitle('Change hotkey')).toBeInTheDocument();
  });

  it('should display placeholder when no value', () => {
    render(
      <HotkeyInput 
        value="" 
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />
    );

    // Check for placeholder text
    expect(screen.getByText('Click to set hotkey')).toBeInTheDocument();
    expect(screen.getByTitle('Change hotkey')).toBeInTheDocument();
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

    // Check if we're in edit mode by looking for the save and cancel buttons
    expect(screen.getByTitle('Save hotkey')).toBeInTheDocument();
    expect(screen.getByTitle('Cancel')).toBeInTheDocument();
    
    // Should show instruction text
    expect(screen.getByText('Press keys to set hotkey')).toBeInTheDocument();
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
    expect(screen.getByText('Press keys to set hotkey')).toBeInTheDocument();

    // Simulate recording a valid combination
    fireEvent.keyDown(document, { key: 'Meta', metaKey: true });
    fireEvent.keyDown(document, { key: 'a', metaKey: true });
    fireEvent.keyUp(document, { key: 'a', metaKey: true });
    fireEvent.keyUp(document, { key: 'Meta' });

    // Wait for the keys to be processed
    await waitFor(() => {
      const saveButton = screen.getByTitle('Save hotkey');
      expect(saveButton).not.toBeDisabled();
    });

    const saveButton = screen.getByTitle('Save hotkey');
    await user.click(saveButton);

    // Should call onChange with the normalized shortcut
    await waitFor(() => {
      expect(mockOnChange).toHaveBeenCalled();
    });
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
    
    // Should still show the original value
    expect(screen.getByText('⌘')).toBeInTheDocument();
    expect(screen.getByText('Space')).toBeInTheDocument();
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

    // Try with only one key
    fireEvent.keyDown(document, { key: 'a' });
    fireEvent.keyUp(document, { key: 'a' });

    // Should show validation error
    await waitFor(() => {
      expect(screen.getByText(/Minimum 2 key\(s\) required/)).toBeInTheDocument();
    });
    
    // Save button should be disabled
    const saveButton = screen.getByTitle('Save hotkey');
    expect(saveButton).toBeDisabled();
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

    // Start recording by pressing some keys
    fireEvent.keyDown(document, { key: 'Meta', metaKey: true });
    
    // Press Escape to cancel recording
    fireEvent.keyDown(document, { key: 'Escape' });

    // Should return to display mode
    await waitFor(() => {
      expect(screen.getByTitle('Change hotkey')).toBeInTheDocument();
    });
    
    expect(mockOnChange).not.toHaveBeenCalled();
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

    // Record only one key
    fireEvent.keyDown(document, { key: 'a' });
    fireEvent.keyUp(document, { key: 'a' });

    await waitFor(() => {
      const saveButton = screen.getByTitle('Save hotkey');
      expect(saveButton).toBeDisabled();
    });
  });

  it('should display correctly formatted shortcuts', () => {
    render(
      <HotkeyInput 
        value="CommandOrControl+Alt+Delete" 
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />
    );

    // On Mac, should show Mac symbols
    expect(screen.getByText('⌘')).toBeInTheDocument();
    expect(screen.getByText('⌥')).toBeInTheDocument();
    expect(screen.getByText('⌦')).toBeInTheDocument();
  });

  it('should handle multi-modifier combinations', async () => {
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

    // Simulate Cmd+Shift+A
    fireEvent.keyDown(document, { key: 'Meta', metaKey: true });
    fireEvent.keyDown(document, { key: 'Shift', metaKey: true, shiftKey: true });
    fireEvent.keyDown(document, { key: 'a', metaKey: true, shiftKey: true });
    fireEvent.keyUp(document, { key: 'a' });
    fireEvent.keyUp(document, { key: 'Shift' });
    fireEvent.keyUp(document, { key: 'Meta' });

    // Save button should be enabled for valid combination
    await waitFor(() => {
      const saveButton = screen.getByTitle('Save hotkey');
      expect(saveButton).not.toBeDisabled();
    });
  });

});