import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ModelCard } from './ModelCard';
import { ModelInfo } from '@/types';

describe('ModelCard', () => {
  const mockModel: ModelInfo = {
    name: 'base',
    display_name: 'Base',
    size: 157286400, // 150MB
    url: 'https://example.com/model.bin',
    downloaded: false,
    speed_score: 7,
    accuracy_score: 5,
  };

  const mockOnDownload = vi.fn();
  const mockOnDelete = vi.fn();
  const mockOnCancelDownload = vi.fn();
  const mockOnSelect = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should render model information correctly', () => {
    render(
      <ModelCard
        name="base"
        model={mockModel}
        onDownload={mockOnDownload}
        onDelete={mockOnDelete}
        onSelect={mockOnSelect}
      />
    );

    expect(screen.getByText('Base')).toBeInTheDocument(); // Uses display_name
  });

  it('should show download button when not downloaded', () => {
    render(
      <ModelCard
        name="base"
        model={mockModel}
        onDownload={mockOnDownload}
        onDelete={mockOnDelete}
        onSelect={mockOnSelect}
      />
    );

    // The button has download icon and text
    const downloadButton = screen.getByRole('button');
    expect(downloadButton).toHaveTextContent('Download');
    expect(downloadButton.querySelector('svg')).toBeInTheDocument();
  });

  it('should show delete button when downloaded', () => {
    const downloadedModel = { ...mockModel, downloaded: true };
    
    render(
      <ModelCard
        name="base"
        model={downloadedModel}
        onDownload={mockOnDownload}
        onDelete={mockOnDelete}
        onSelect={mockOnSelect}
      />
    );

    // Find the delete button by looking for buttons with X icon
    const buttons = screen.getAllByRole('button');
    const deleteButton = buttons.find(btn => btn.querySelector('.lucide-x'));
    expect(deleteButton).toBeInTheDocument();
  });

  it('should show progress bar when downloading', () => {
    render(
      <ModelCard
        name="base"
        model={mockModel}
        downloadProgress={45}
        onDownload={mockOnDownload}
        onDelete={mockOnDelete}
        onCancelDownload={mockOnCancelDownload}
        onSelect={mockOnSelect}
      />
    );

    expect(screen.getByText('45%')).toBeInTheDocument();
    // The cancel button only has an icon, not text
    const cancelButton = screen.getByRole('button');
    expect(cancelButton.querySelector('svg')).toBeInTheDocument();
  });

  it('should call onDownload when download button clicked', async () => {
    const user = userEvent.setup();
    
    render(
      <ModelCard
        name="base"
        model={mockModel}
        onDownload={mockOnDownload}
        onDelete={mockOnDelete}
        onSelect={mockOnSelect}
      />
    );

    const downloadButton = screen.getByRole('button');
    await user.click(downloadButton);

    expect(mockOnDownload).toHaveBeenCalledWith('base');
  });

  it('should call onDelete when delete button clicked', async () => {
    const user = userEvent.setup();
    const downloadedModel = { ...mockModel, downloaded: true };
    
    render(
      <ModelCard
        name="base"
        model={downloadedModel}
        onDownload={mockOnDownload}
        onDelete={mockOnDelete}
        onSelect={mockOnSelect}
      />
    );

    // Find the delete button by looking for buttons with X icon
    const buttons = screen.getAllByRole('button');
    const deleteButton = buttons.find(btn => btn.querySelector('.lucide-x'));
    if (!deleteButton) throw new Error('Delete button not found');
    await user.click(deleteButton);

    expect(mockOnDelete).toHaveBeenCalledWith('base');
  });

  it('should call onCancelDownload when cancel button clicked', async () => {
    const user = userEvent.setup();
    
    render(
      <ModelCard
        name="base"
        model={mockModel}
        downloadProgress={45}
        onDownload={mockOnDownload}
        onDelete={mockOnDelete}
        onCancelDownload={mockOnCancelDownload}
        onSelect={mockOnSelect}
      />
    );

    const cancelButton = screen.getByRole('button');
    await user.click(cancelButton);

    expect(mockOnCancelDownload).toHaveBeenCalledWith('base');
  });

  it('should be clickable when downloaded', () => {
    const downloadedModel = { ...mockModel, downloaded: true };
    
    render(
      <ModelCard
        name="base"
        model={downloadedModel}
        showSelectButton={true}
        onDownload={mockOnDownload}
        onDelete={mockOnDelete}
        onSelect={mockOnSelect}
      />
    );

    // The card itself becomes clickable when downloaded
    const card = document.querySelector('[data-slot="card"]');
    expect(card).toHaveClass('cursor-pointer');
  });

  it('should show selected state correctly', () => {
    const downloadedModel = { ...mockModel, downloaded: true };
    
    render(
      <ModelCard
        name="base"
        model={downloadedModel}
        isSelected={true}
        showSelectButton={true}
        onDownload={mockOnDownload}
        onDelete={mockOnDelete}
        onSelect={mockOnSelect}
      />
    );

    // When selected, the card has special styling
    const card = document.querySelector('[data-slot="card"]');
    expect(card).toHaveClass('border-primary');
    expect(card).toHaveClass('bg-primary/5');
  });

  it('should format large file sizes correctly', () => {
    const largeModel = { ...mockModel, name: 'large', display_name: 'Large', size: 2147483648 }; // 2GB
    
    render(
      <ModelCard
        name="large"
        model={largeModel}
        onDownload={mockOnDownload}
        onDelete={mockOnDelete}
        onSelect={mockOnSelect}
      />
    );

    // Check the displayed name
    expect(screen.getByText('Large')).toBeInTheDocument();
    // Check that model shows size properly in the stats
    expect(screen.getByText('2.0 GB')).toBeInTheDocument();
  });

  it('should display model names correctly', () => {
    const enModel = { ...mockModel, name: 'base.en', display_name: 'Base (English)' };
    render(
      <ModelCard
        name="base.en"
        model={enModel}
        onDownload={mockOnDownload}
        onDelete={mockOnDelete}
        onSelect={mockOnSelect}
      />
    );

    // The component shows the display_name
    expect(screen.getByText('Base (English)')).toBeInTheDocument();
  });
});