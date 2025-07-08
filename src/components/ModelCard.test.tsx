import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ModelCard } from './ModelCard';
import { ModelInfo } from '@/types';

describe('ModelCard', () => {
  const mockModel: ModelInfo = {
    name: 'base',
    size: 157286400, // 150MB
    url: 'https://example.com/model.bin',
    sha256: 'abc123',
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

    expect(screen.getByText('Base')).toBeInTheDocument(); // Name is capitalized
    expect(screen.getByText(/Speed: 7\/10 • Accuracy: 5\/10 • 150 MB/)).toBeInTheDocument();
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

    // The button only has an icon, not text
    const downloadButton = screen.getByRole('button');
    expect(downloadButton.querySelector('svg')).toBeInTheDocument();
    expect(downloadButton).toHaveClass('h-8', 'w-8');
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

    // Find the delete button by looking for buttons with Trash2 icon
    const buttons = screen.getAllByRole('button');
    const deleteButton = buttons.find(btn => btn.querySelector('.lucide-trash2'));
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
      />
    );

    // Find the delete button by looking for buttons with Trash2 icon
    const buttons = screen.getAllByRole('button');
    const deleteButton = buttons.find(btn => btn.querySelector('.lucide-trash2'));
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
      />
    );

    const cancelButton = screen.getByRole('button');
    await user.click(cancelButton);

    expect(mockOnCancelDownload).toHaveBeenCalledWith('base');
  });

  it('should show select button when downloaded and callback provided', () => {
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

    // The select button has a CheckCircle icon
    const buttons = screen.getAllByRole('button');
    const selectButton = buttons.find(btn => btn.querySelector('.lucide-circle-check'));
    expect(selectButton).toBeInTheDocument();
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

    // When selected, it shows a CheckCircle icon (not a button)
    const checkIcon = document.querySelector('.lucide-circle-check.text-primary');
    expect(checkIcon).toBeInTheDocument();
  });

  it('should format large file sizes correctly', () => {
    const largeModel = { ...mockModel, size: 2147483648 }; // 2GB
    
    render(
      <ModelCard
        name="large"
        model={largeModel}
        onDownload={mockOnDownload}
        onDelete={mockOnDelete}
      />
    );

    expect(screen.getByText(/Speed: 7\/10 • Accuracy: 5\/10 • 2\.0 GB/)).toBeInTheDocument();
  });

  it('should display model names correctly', () => {
    render(
      <ModelCard
        name="base.en"
        model={mockModel}
        onDownload={mockOnDownload}
        onDelete={mockOnDelete}
        onSelect={mockOnSelect}
      />
    );

    // The component doesn't show "English-only" but shows the formatted name
    expect(screen.getByText('base.en')).toBeInTheDocument();
  });
});