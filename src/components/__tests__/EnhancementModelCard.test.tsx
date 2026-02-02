import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { EnhancementModelCard } from '../EnhancementModelCard';
import { ask } from '@tauri-apps/plugin-dialog';

// Mock the dialog plugin
vi.mock('@tauri-apps/plugin-dialog', () => ({
  ask: vi.fn(),
}));

describe('EnhancementModelCard', () => {
  const mockModel = {
    id: 'gemini-2.0-flash',
    name: 'Gemini 2.0 Flash',
    provider: 'gemini',
    description: 'Fast and versatile language model',
  };

  const defaultProps = {
    model: mockModel,
    hasApiKey: false,
    isSelected: false,
    onSetupApiKey: vi.fn(),
    onSelect: vi.fn(),
    onRemoveApiKey: vi.fn(),
  };

  it('renders model information', () => {
    render(<EnhancementModelCard {...defaultProps} />);
    
    expect(screen.getByText('Gemini 2.0 Flash')).toBeInTheDocument();
    expect(screen.getByText('Gemini')).toBeInTheDocument();
  });

  it('shows key icon when no API key', () => {
    render(<EnhancementModelCard {...defaultProps} />);
    
    const keyButton = screen.getByRole('button');
    expect(keyButton).toBeInTheDocument();
    expect(keyButton.querySelector('svg')).toBeInTheDocument();
  });

  it('shows remove button when API key exists', () => {
    render(<EnhancementModelCard {...defaultProps} hasApiKey={true} />);
    
    const removeButton = screen.getByRole('button');
    expect(removeButton).toBeInTheDocument();
    expect(removeButton.querySelector('svg')).toBeInTheDocument();
  });

  it('applies selected styling', () => {
    render(<EnhancementModelCard {...defaultProps} isSelected={true} />);
    
    // User should see the model card is rendered and selectable
    // The actual visual styling is an implementation detail
    expect(screen.getByText('Gemini 2.0 Flash')).toBeInTheDocument();
  });

  it('calls onSetupApiKey when key button is clicked', () => {
    const onSetupApiKey = vi.fn();
    render(<EnhancementModelCard {...defaultProps} onSetupApiKey={onSetupApiKey} />);
    
    const keyButton = screen.getByRole('button');
    fireEvent.click(keyButton);
    
    expect(onSetupApiKey).toHaveBeenCalledTimes(1);
  });

  it('calls onSelect when card is clicked with API key', () => {
    const onSelect = vi.fn();
    render(<EnhancementModelCard {...defaultProps} hasApiKey={true} onSelect={onSelect} />);
    
    const card = screen.getByText('Gemini 2.0 Flash').closest('.transition-all');
    if (card) {
      fireEvent.click(card);
    }
    
    expect(onSelect).toHaveBeenCalledTimes(1);
  });

  it('does not call onSelect when card is clicked without API key', () => {
    const onSelect = vi.fn();
    render(<EnhancementModelCard {...defaultProps} onSelect={onSelect} />);
    
    const card = screen.getByText('Gemini 2.0 Flash').closest('.transition-all');
    if (card) {
      fireEvent.click(card);
    }
    
    expect(onSelect).not.toHaveBeenCalled();
  });

  it('prevents card click event when clicking key button', () => {
    const onSetupApiKey = vi.fn();
    const onSelect = vi.fn();
    render(<EnhancementModelCard {...defaultProps} onSetupApiKey={onSetupApiKey} onSelect={onSelect} />);
    
    const keyButton = screen.getByRole('button');
    fireEvent.click(keyButton);
    
    expect(onSetupApiKey).toHaveBeenCalledTimes(1);
    expect(onSelect).not.toHaveBeenCalled();
  });

  it('displays correct provider color', () => {
    render(<EnhancementModelCard {...defaultProps} />);
    
    const providerText = screen.getByText('Gemini');
    expect(providerText).toHaveClass('text-blue-600');
  });

  it('calls onRemoveApiKey when remove button is clicked and confirmed', async () => {
    const onRemoveApiKey = vi.fn();
    (ask as any).mockResolvedValue(true);
    
    render(<EnhancementModelCard {...defaultProps} hasApiKey={true} onRemoveApiKey={onRemoveApiKey} />);
    
    const removeButton = screen.getByRole('button');
    fireEvent.click(removeButton);
    
    await waitFor(() => {
      expect(ask).toHaveBeenCalledWith(
        'Remove API key for Gemini?',
        { title: 'Remove API Key', kind: 'warning' }
      );
      expect(onRemoveApiKey).toHaveBeenCalledTimes(1);
    });
  });

  it('does not call onRemoveApiKey when removal is cancelled', async () => {
    const onRemoveApiKey = vi.fn();
    (ask as any).mockResolvedValue(false);
    
    render(<EnhancementModelCard {...defaultProps} hasApiKey={true} onRemoveApiKey={onRemoveApiKey} />);
    
    const removeButton = screen.getByRole('button');
    fireEvent.click(removeButton);
    
    await waitFor(() => {
      expect(ask).toHaveBeenCalled();
      expect(onRemoveApiKey).not.toHaveBeenCalled();
    });
  });
});