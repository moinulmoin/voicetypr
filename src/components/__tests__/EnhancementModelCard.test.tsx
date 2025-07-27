import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { EnhancementModelCard } from '../EnhancementModelCard';

describe('EnhancementModelCard', () => {
  const mockModel = {
    id: 'llama-3.3-70b-versatile',
    name: 'Llama 3.3 70B',
    provider: 'groq',
    description: 'Fast and versatile language model',
  };

  const defaultProps = {
    model: mockModel,
    hasApiKey: false,
    isSelected: false,
    onSetupApiKey: vi.fn(),
    onSelect: vi.fn(),
  };

  it('renders model information', () => {
    render(<EnhancementModelCard {...defaultProps} />);
    
    expect(screen.getByText('Llama 3.3 70B')).toBeInTheDocument();
    expect(screen.getByText('Fast and versatile language model')).toBeInTheDocument();
    expect(screen.getByText('Groq')).toBeInTheDocument();
  });

  it('shows key icon when no API key', () => {
    render(<EnhancementModelCard {...defaultProps} />);
    
    const keyButton = screen.getByRole('button');
    expect(keyButton).toBeInTheDocument();
    expect(keyButton.querySelector('svg')).toBeInTheDocument();
  });

  it('shows ready status when API key exists', () => {
    render(<EnhancementModelCard {...defaultProps} hasApiKey={true} />);
    
    expect(screen.getByText('Ready')).toBeInTheDocument();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });

  it('applies selected styling', () => {
    render(<EnhancementModelCard {...defaultProps} isSelected={true} />);
    
    const card = screen.getByText('Llama 3.3 70B').closest('.transition-all');
    expect(card).toHaveClass('border-primary');
    expect(card).toHaveClass('bg-primary/5');
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
    
    const card = screen.getByText('Llama 3.3 70B').closest('.transition-all');
    if (card) {
      fireEvent.click(card);
    }
    
    expect(onSelect).toHaveBeenCalledTimes(1);
  });

  it('does not call onSelect when card is clicked without API key', () => {
    const onSelect = vi.fn();
    render(<EnhancementModelCard {...defaultProps} onSelect={onSelect} />);
    
    const card = screen.getByText('Llama 3.3 70B').closest('.transition-all');
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

  it('displays correct provider badge styling', () => {
    render(<EnhancementModelCard {...defaultProps} />);
    
    const badge = screen.getByText('Groq');
    expect(badge).toHaveClass('bg-orange-100');
    expect(badge).toHaveClass('text-orange-700');
  });
});